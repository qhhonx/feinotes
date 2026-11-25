//! Three-way merge of two versions of a document that share an ancestor, by
//! block (docs/design/sync.md §3.2). Sync calls it with the base it last saw
//! (iCloud) or the merge base (Git); nothing here knows where they came from.
//!
//! Blocks have no stable ids on disk, so each side is aligned with the base by
//! content: an LCS on exact blocks, then, in the gaps between, a block of the
//! same kind whose text is similar enough counts as that block edited rather
//! than as one removed and another added. A base block found on both sides is
//! an anchor and merges on its own (its own text and attributes, then its
//! children the same way); between anchors, the side that left its stretch as
//! the base had it takes the other's, and where both changed a stretch
//! differently the result keeps ours and records a conflict. Both sides adding
//! at the same spot keeps both, ours first: nothing is lost and nothing to pick.
//! Text inside one block is not merged (D14): both editing a paragraph is a
//! conflict.

use crate::docxml::serialize;
use crate::model::{Block, Document};
use std::mem::discriminant;

#[derive(Clone, Debug, PartialEq)]
pub enum Merge {
    Clean(Document),
    /// `merged` keeps ours wherever both changed the same thing differently.
    Conflicts { merged: Document, regions: Vec<Conflict> },
}

impl Merge {
    pub fn document(&self) -> &Document {
        match self { Merge::Clean(d) => d, Merge::Conflicts { merged, .. } => merged }
    }
}

/// Where the merged document kept ours over a different change of theirs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    Title,
    Icon,
    Cover,
    /// `len` blocks from `start` among the children of the block at `parent`
    /// (indices from the top; empty: the document's own blocks), in the merged
    /// document. `len` is 0 where ours removed what theirs changed.
    Blocks { parent: Vec<usize>, start: usize, len: usize },
}

pub fn merge3(base: &Document, ours: &Document, theirs: &Document) -> Merge {
    let mut regions = vec![];
    let title = field(&base.title, &ours.title, &theirs.title, Conflict::Title, &mut regions);
    let icon = field(&base.icon, &ours.icon, &theirs.icon, Conflict::Icon, &mut regions);
    let cover = field(&base.cover, &ours.cover, &theirs.cover, Conflict::Cover, &mut regions);
    let mut blocks = seq(&base.blocks, &ours.blocks, &theirs.blocks, &[], &mut regions);
    if blocks.is_empty() { blocks.push(Block::text("")) }
    let merged = Document { title, icon, cover, blocks };
    if regions.is_empty() { Merge::Clean(merged) } else { Merge::Conflicts { merged, regions } }
}

/// A document-level attribute: the side that changed it, else ours and a conflict.
fn field<T: PartialEq + Clone>(b: &T, o: &T, t: &T, c: Conflict, regions: &mut Vec<Conflict>) -> T {
    if o == b { return t.clone() }
    if t != b && t != o { regions.push(c) }
    o.clone()
}

/// A block's content without its ids: what it would be written as.
fn key(b: &Block) -> String {
    serialize(&Document { title: Default::default(), icon: None, cover: None, blocks: vec![b.clone()] })
}

/// The block without its children: its own text, kind and attributes.
fn head(b: &Block) -> String {
    let mut h = b.clone();
    h.children.clear();
    key(&h)
}

/// Beyond this many blocks in one stretch the LCS table is not worth it; the
/// stretch is then aligned by its common ends only.
const MAX_LCS: usize = 3000;
/// Dice coefficient of character bigrams at or above which a block of the same
/// kind counts as the same block edited.
const SIMILAR: f32 = 0.5;

/// For each base block, its counterpart on one side, in order.
fn align(base: &[Block], side: &[Block], bk: &[String], sk: &[String]) -> Vec<Option<usize>> {
    let (n, m) = (base.len(), side.len());
    let mut map = vec![None; n];
    let mut pre = 0;
    while pre < n && pre < m && bk[pre] == sk[pre] { map[pre] = Some(pre); pre += 1 }
    let mut suf = 0;
    while suf < n - pre && suf < m - pre && bk[n - 1 - suf] == sk[m - 1 - suf] { map[n - 1 - suf] = Some(m - 1 - suf); suf += 1 }
    let (a, b) = (pre..n - suf, pre..m - suf);
    let mut exact: Vec<(usize, usize)> = vec![];
    if a.len() * b.len() <= MAX_LCS * MAX_LCS / 4 && !a.is_empty() && !b.is_empty() {
        let (h, w) = (a.len(), b.len() + 1);
        let mut dp = vec![0u32; (h + 1) * w];
        for i in (0..h).rev() {
            for j in (0..b.len()).rev() {
                dp[i * w + j] = if bk[a.start + i] == sk[b.start + j] { dp[(i + 1) * w + j + 1] + 1 } else { dp[(i + 1) * w + j].max(dp[i * w + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < h && j < b.len() {
            if bk[a.start + i] == sk[b.start + j] { exact.push((a.start + i, b.start + j)); i += 1; j += 1 }
            else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] { i += 1 }
            else { j += 1 }
        }
    }
    // The gaps between exact matches: pair edited blocks.
    let mut from = (a.start, b.start);
    exact.push((a.end, b.end));
    for &(i, j) in &exact {
        pair_edited(base, side, from.0..i, from.1..j, &mut map);
        if i < a.end { map[i] = Some(j) }
        from = (i + 1, j + 1);
    }
    map
}

/// As many removed as added blocks, of the same kinds in the same order: each
/// was edited in place (as `changes.rs` pairs them). Otherwise, in order, each
/// removed block takes the first later added one of the same kind with
/// similar text.
fn pair_edited(base: &[Block], side: &[Block], removed: std::ops::Range<usize>, added: std::ops::Range<usize>, map: &mut [Option<usize>]) {
    if removed.len() == added.len() && removed.clone().zip(added.clone()).all(|(i, j)| discriminant(&base[i].kind) == discriminant(&side[j].kind)) {
        removed.zip(added).for_each(|(i, j)| map[i] = Some(j));
        return
    }
    let mut next = added.start;
    for i in removed {
        let found = (next..added.end).find(|&j| discriminant(&base[i].kind) == discriminant(&side[j].kind) && similar(&base[i], &side[j]) >= SIMILAR);
        if let Some(j) = found { map[i] = Some(j); next = j + 1 }
    }
}

fn similar(a: &Block, b: &Block) -> f32 {
    let (x, y) = (a.text.plain_text(), b.text.plain_text());
    if x.trim().is_empty() && y.trim().is_empty() {
        // Images, dividers, containers: their own text is empty; go by their children.
        let (p, q) = (a.children.iter().map(|c| c.text.plain_text()).collect::<String>(), b.children.iter().map(|c| c.text.plain_text()).collect::<String>());
        return if p.is_empty() && q.is_empty() { 1. } else { dice(&p, &q) }
    }
    dice(&x, &y)
}

fn dice(x: &str, y: &str) -> f32 {
    // Padded, so one-character paragraphs have bigrams too.
    let grams = |s: &str| { let c: Vec<char> = std::iter::once('\u{2}').chain(s.chars()).chain(std::iter::once('\u{3}')).collect(); let mut g: Vec<(char, char)> = c.windows(2).map(|w| (w[0], w[1])).collect(); g.sort_unstable(); g };
    let (a, b) = (grams(x), grams(y));
    let (mut i, mut j, mut common) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => { common += 1; i += 1; j += 1 }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    2. * common as f32 / (a.len() + b.len()) as f32
}

fn seq(base: &[Block], ours: &[Block], theirs: &[Block], parent: &[usize], regions: &mut Vec<Conflict>) -> Vec<Block> {
    let (bk, ok, tk): (Vec<String>, Vec<String>, Vec<String>) = (base.iter().map(key).collect(), ours.iter().map(key).collect(), theirs.iter().map(key).collect());
    let (om, tm) = (align(base, ours, &bk, &ok), align(base, theirs, &bk, &tk));
    let mut out: Vec<Block> = vec![];
    let (mut b0, mut o0, mut t0) = (0, 0, 0);
    let anchors = (0..base.len()).filter_map(|i| Some((i, om[i]?, tm[i]?)));
    for (i, o, t) in anchors.chain(std::iter::once((base.len(), ours.len(), theirs.len()))) {
        // Gap: blocks that are not on both sides.
        let (gb, go, gt) = (b0..i, o0..o, t0..t);
        let same = |x: &[String], y: &[String]| x == y;
        if same(&ok[go.clone()], &bk[gb.clone()]) { out.extend_from_slice(&theirs[gt]) }
        else if same(&tk[gt.clone()], &bk[gb.clone()]) || same(&ok[go.clone()], &tk[gt.clone()]) { out.extend_from_slice(&ours[go]) }
        else if gb.is_empty() { out.extend_from_slice(&ours[go]); out.extend_from_slice(&theirs[gt]) }
        else {
            regions.push(Conflict::Blocks { parent: parent.to_vec(), start: out.len(), len: go.len() });
            out.extend_from_slice(&ours[go]);
        }
        if i == base.len() { break }
        let at = out.len();
        out.push(block(&base[i], &ours[o], &theirs[t], (&bk[i], &ok[o], &tk[t]), &[parent, &[at]].concat(), regions));
        (b0, o0, t0) = (i + 1, o + 1, t + 1);
    }
    out
}

/// One base block found on both sides. `path`: where the result goes.
fn block(b: &Block, o: &Block, t: &Block, (bk, ok, tk): (&str, &str, &str), path: &[usize], regions: &mut Vec<Conflict>) -> Block {
    if tk == bk || ok == tk { return o.clone() }
    if ok == bk { return t.clone() }
    let (hb, ho, ht) = (head(b), head(o), head(t));
    let mut r = if ho == hb { t.clone() } else { o.clone() };
    let one = |r: &mut Vec<Conflict>| r.push(Conflict::Blocks { parent: path[..path.len() - 1].to_vec(), start: path[path.len() - 1], len: 1 });
    let mut nested = vec![];
    let children = seq(&b.children, &o.children, &t.children, path, &mut nested);
    if !children.is_empty() && !(r.kind.nests() || r.kind.is_container()) {
        // The head that won takes no children (a paragraph made a code block
        // while the other side indented under it): keep ours whole.
        one(regions);
        return o.clone();
    }
    if ho != hb && ht != hb && ho != ht { one(regions) }
    regions.extend(nested);
    r.id = o.id;
    r.children = children;
    // Each side removed a different last child: a container is never empty.
    if r.children.is_empty() && r.kind.is_container() { r.children.push(Block::text("")) }
    r
}

#[cfg(test)]
pub(crate) mod tests;
