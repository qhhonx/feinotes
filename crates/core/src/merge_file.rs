//! Three-way merge of a whole file's bytes (docs/design/sync.md §3.2): the
//! layer between the sync engine, which only knows files, and `merge3`, which
//! only knows documents. Equal bytes are decided without parsing; XML and
//! Markdown files are parsed, merged by block and written back; anything else,
//! or anything that does not parse, cannot be merged and keeps ours whole.

use crate::docxml;
use crate::markdown::{self, Options};
use crate::merge::{merge3, Conflict, Merge};
use crate::model::Document;
use crate::store::DocKind;
use crate::time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Xml, Markdown, Other }

impl Kind {
    /// By extension, the way the library tells its documents apart.
    pub fn of_name(name: &str) -> Kind {
        match DocKind::of_name(name) { Some((DocKind::Doc, false)) => Kind::Xml, Some((DocKind::Doc, true)) => Kind::Markdown, _ => Kind::Other }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FileMerge {
    Clean(Vec<u8>),
    /// `merged` is ours plus every change that merged on its own; `theirs` is
    /// their file as it came, for the caller to keep as a conflict copy.
    /// `regions` say where `merged` kept ours over theirs; it is empty when the
    /// whole file conflicts (no base, not a document, or one side unreadable),
    /// then `merged` is ours untouched.
    Conflicts { merged: Vec<u8>, regions: Vec<Conflict>, theirs: Vec<u8> },
}

pub fn merge_file(base: Option<&[u8]>, ours: &[u8], theirs: &[u8], kind: Kind) -> FileMerge {
    if ours == theirs || base == Some(ours) { return FileMerge::Clean(theirs.to_vec()) }
    if base == Some(theirs) { return FileMerge::Clean(ours.to_vec()) }
    let whole = || FileMerge::Conflicts { merged: ours.to_vec(), regions: vec![], theirs: theirs.to_vec() };
    let Some(base) = base else { return whole() };
    let (Ok(b), Ok(o), Ok(t)) = (std::str::from_utf8(base), std::str::from_utf8(ours), std::str::from_utf8(theirs)) else { return whole() };
    let parse = |s: &str| match kind {
        Kind::Xml => docxml::parse(s).ok(),
        Kind::Markdown => Some(markdown::parse_file(s, "", None)),
        Kind::Other => None,
    };
    let (Some(bd), Some(od), Some(td)) = (parse(b), parse(o), parse(t)) else { return whole() };
    let (doc, regions) = match merge3(&bd, &od, &td) { Merge::Clean(d) => (d, vec![]), Merge::Conflicts { merged, regions } => (merged, regions) };
    let out = write(&doc, o, kind).into_bytes();
    if regions.is_empty() { FileMerge::Clean(out) } else { FileMerge::Conflicts { merged: out, regions, theirs: theirs.to_vec() } }
}

/// Markdown keeps our front matter and, through `original`, every line of our
/// file the merge left alone as it was written (a BOM is dropped).
fn write(doc: &Document, ours: &str, kind: Kind) -> String {
    if kind == Kind::Xml { return docxml::serialize(doc) }
    let ours = ours.trim_start_matches('\u{feff}');
    markdown::write_file_with(doc, markdown::front_matter(ours).0, &Options { original: Some(ours), ..Default::default() }).text
}

/// The name for the conflict copy of `stem.ext`: `周报（冲突 · iPad · 11-22 14.30）.xml`,
/// or `Weekly (conflict · iPad · 11-22 14.30).xml`. `at` is unix seconds, shown
/// in local time. The spec's `14:30` is written `14.30`: `:` is not allowed in
/// Windows file names and is `/` to Finder. The device name loses every
/// character a file name may not hold (`/ \ : * ? " < > |`, controls) to `-`.
/// Whether the name is free is the caller's (`Store::free_name`).
pub fn conflict_copy_name(stem: &str, ext: &str, device: &str, at: u64, english: bool) -> String {
    let device: String = device.trim().chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let (_, m, d, h, mi) = time::civil(at);
    let tag = format!("{device} · {m:02}-{d:02} {h:02}.{mi:02}");
    let name = if english { format!("{stem} (conflict · {tag})") } else { format!("{stem}（冲突 · {tag}）") };
    let ext = ext.trim_start_matches('.');
    if ext.is_empty() { name } else { format!("{name}.{ext}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, BlockKind, RichText};

    fn xml(ps: &[&str]) -> Vec<u8> { format!("<title>t</title>\n{}", ps.iter().map(|p| format!("<p>{p}</p>\n")).collect::<String>()).into_bytes() }
    fn clean(m: FileMerge) -> Vec<u8> { match m { FileMerge::Clean(b) => b, FileMerge::Conflicts { regions, .. } => panic!("conflicts {regions:?}") } }

    #[test]
    fn equal_bytes_never_parse() {
        // Not valid in any format: only the byte comparisons can answer.
        let (a, b, c): (&[u8], &[u8], &[u8]) = (b"\xff<a", b"\xffb", b"\xffc");
        assert_eq!(merge_file(Some(a), b, b, Kind::Xml), FileMerge::Clean(b.to_vec()));
        assert_eq!(merge_file(Some(a), a, c, Kind::Xml), FileMerge::Clean(c.to_vec()));
        assert_eq!(merge_file(Some(a), b, a, Kind::Xml), FileMerge::Clean(b.to_vec()));
        assert_eq!(merge_file(None, b, b, Kind::Other), FileMerge::Clean(b.to_vec()));
    }

    #[test]
    fn no_base_or_not_a_document_keeps_ours_whole() {
        let whole = |m: FileMerge, o: &[u8], t: &[u8]| assert_eq!(m, FileMerge::Conflicts { merged: o.to_vec(), regions: vec![], theirs: t.to_vec() });
        let (o, t) = (xml(&["一"]), xml(&["二"]));
        whole(merge_file(None, &o, &t, Kind::Xml), &o, &t);
        let (b, o, t): (&[u8], &[u8], &[u8]) = (b"\0\x01", b"\0\x02", b"\0\x03");
        whole(merge_file(Some(b), o, t, Kind::Other), o, t);
        let (b, o, t): (&[u8], &[u8], &[u8]) = (b"a\n", b"\xffb\n", b"c\n");
        whole(merge_file(Some(b), o, t, Kind::Markdown), o, t);
        let (b, o, t) = (xml(&["一"]), b"<p>unclosed".to_vec(), xml(&["二"]));
        whole(merge_file(Some(&b), &o, &t, Kind::Xml), &o, &t);
    }

    #[test]
    fn xml_edits_to_different_paragraphs_both_land() {
        let base = xml(&["一", "二", "三", "四"]);
        let (ours, theirs) = (xml(&["一 改", "二", "三", "四"]), xml(&["一", "二", "三", "四 改了"]));
        assert_eq!(clean(merge_file(Some(&base), &ours, &theirs, Kind::Xml)), xml(&["一 改", "二", "三", "四 改了"]));
    }

    #[test]
    fn xml_same_paragraph_keeps_ours_and_theirs_apart() {
        let base = xml(&["一", "今天天气好", "三"]);
        let (ours, theirs) = (xml(&["一", "今天上海天气好", "三"]), xml(&["一", "今天天气好！", "三"]));
        let FileMerge::Conflicts { merged, regions, theirs: t } = merge_file(Some(&base), &ours, &theirs, Kind::Xml) else { panic!() };
        assert_eq!((merged, t), (ours, theirs));
        assert_eq!(regions, vec![Conflict::Blocks { parent: vec![], start: 1, len: 1 }]);
    }

    const MD: &str = "# 周报\n\nfirst paragraph with **bold** text.\n\n- item one\n- item two\n\nsecond paragraph with a [link](https://example.com).\n\n```rust\nfn main() {}\n```\n\nlast paragraph here.\n";

    #[test]
    fn markdown_edits_to_different_paragraphs_both_land_byte_exact() {
        let with_front = format!("---\ntags: [a]\n---\n\n{MD}");
        for base in [MD.to_string(), with_front] {
            let ours = base.replace("first paragraph", "first edited paragraph");
            let theirs = base.replace("last paragraph here", "last paragraph, changed");
            let want = ours.replace("last paragraph here", "last paragraph, changed");
            let out = clean(merge_file(Some(base.as_bytes()), ours.as_bytes(), theirs.as_bytes(), Kind::Markdown));
            assert_eq!(String::from_utf8(out).unwrap(), want);
        }
    }

    #[test]
    fn markdown_same_paragraph_conflicts_and_stays_markdown() {
        let ours = MD.replace("second paragraph", "second paragraph (mine)");
        let theirs = MD.replace("second paragraph", "second paragraph (theirs)").replace("# 周报", "# 周报二");
        let FileMerge::Conflicts { merged, regions, theirs: t } = merge_file(Some(MD.as_bytes()), ours.as_bytes(), theirs.as_bytes(), Kind::Markdown) else { panic!() };
        assert_eq!(t, theirs.as_bytes());
        assert_eq!(regions.len(), 1);
        assert_eq!(String::from_utf8(merged).unwrap(), ours.replace("# 周报", "# 周报二"));
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(["a.xml", "a.MD", "a.markdown", "a.png", "a", ".xml"].map(Kind::of_name), [Kind::Xml, Kind::Markdown, Kind::Markdown, Kind::Other, Kind::Other, Kind::Other]);
    }

    #[test]
    fn conflict_copy_names() {
        let at = time::local_to_unix(2026, 11, 22, 14, 30) as u64;
        assert_eq!(conflict_copy_name("周报", "xml", "iPad", at, false), "周报（冲突 · iPad · 11-22 14.30）.xml");
        assert_eq!(conflict_copy_name("Weekly", ".md", "iPad", at, true), "Weekly (conflict · iPad · 11-22 14.30).md");
        assert_eq!(conflict_copy_name("a", "xml", "Sida's/Mac: 1\n", at, true), "a (conflict · Sida's-Mac- 1 · 11-22 14.30).xml");
        assert_eq!(conflict_copy_name("a", "", "X", at, true), "a (conflict · X · 11-22 14.30)");
    }

    // ------------------------------------------------------------ random merges

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }
        fn below(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize }
        fn chance(&mut self, percent: u64) -> bool { self.next() % 100 < percent }
        fn word(&mut self) -> String { (0..6).map(|_| char::from(b'a' + self.below(26) as u8)).collect() }
    }

    const NESTED: &str = r#"<title>随机 <b>合并</b></title>
<h1>标题一</h1>
<p>普通 <b>加粗</b> <em>斜体</em> 😀</p>
<ul><li>一<ul><li>一.一</li><li>一.二</li></ul></li><li>二</li></ul>
<ol><li>三</li><li>四</li></ol>
<checkbox done="true">完成</checkbox>
<blockquote><p>引用一</p><p>引用二</p></blockquote>
<pre lang="rust"><code>fn main() {}</code></pre>
<hr/>
<callout emoji="💡"><p>提示</p><ul><li>提示列表</li></ul></callout>
<grid><column width-ratio="0.5"><p>左</p></column><column width-ratio="0.5"><p>右</p><p>右二</p></column></grid>
<p></p>
<p>结尾</p>
"#;

    fn paths(blocks: &[Block], at: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, b) in blocks.iter().enumerate() {
            at.push(i);
            out.push(at.clone());
            paths(&b.children, at, out);
            at.pop();
        }
    }

    fn list_at<'a>(blocks: &'a mut Vec<Block>, parent: &[usize]) -> &'a mut Vec<Block> {
        match parent.split_first() { None => blocks, Some((&i, rest)) => list_at(&mut blocks[i].children, rest) }
    }

    /// One random edit anywhere in the document, as a user could make it.
    fn mutate(d: &mut Document, rng: &mut Rng) {
        let mut all = vec![];
        paths(&d.blocks, &mut vec![], &mut all);
        let p = all[rng.below(all.len())].clone();
        let (parent, i) = (p[..p.len() - 1].to_vec(), p[p.len() - 1]);
        let list = list_at(&mut d.blocks, &parent);
        let takes_text = |l: &Vec<Block>| parent.is_empty() || !matches!(l.first().map(|b| &b.kind), Some(BlockKind::Column { .. }));
        match rng.below(4) {
            0 | 1 if list[i].kind.nests() || matches!(list[i].kind, BlockKind::Code { .. }) => {
                let w = rng.word();
                let b = &mut list[i];
                b.text = if rng.chance(50) { RichText::plain(format!("{} {w}", b.text.plain_text())) } else { RichText::plain(w) };
            }
            2 if takes_text(list) => { let at = rng.below(list.len() + 1); list.insert(at, Block::text(&rng.word())) }
            3 if !matches!(list[i].kind, BlockKind::Column { .. }) && list.len() > 1 => { list.remove(i); }
            _ => {}
        }
    }

    fn edited(base: &Document, rng: &mut Rng) -> Document {
        let mut d = base.clone();
        for _ in 0..rng.below(4) { mutate(&mut d, rng) }
        d
    }

    #[test]
    fn random_xml_merges_are_files_that_read_back() {
        let seeds = std::env::var("FDL_FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let base0 = docxml::parse(NESTED).unwrap();
        for seed in 1..=seeds {
            let mut rng = Rng(seed * 0x9e37_79b9 + 7);
            let base = edited(&base0, &mut rng);
            let (ours, theirs) = (edited(&base, &mut rng), edited(&base, &mut rng));
            let [b, o, t] = [&base, &ours, &theirs].map(|d| docxml::serialize(d).into_bytes());
            let out = match merge_file(Some(&b), &o, &t, Kind::Xml) {
                FileMerge::Clean(x) => x,
                FileMerge::Conflicts { merged, theirs, .. } => { assert_eq!(theirs, t, "seed {seed}: theirs altered"); merged }
            };
            let text = String::from_utf8(out).unwrap();
            assert_eq!(docxml::serialize(&docxml::parse(&text).unwrap()), text, "seed {seed}: does not round-trip");
        }
    }
}
