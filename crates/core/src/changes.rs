//! Block-level changes between two versions, for tinting one of them in place.
//!
//! The line diff in `diff.rs` answers "what text changed"; this answers "which
//! blocks do I paint". Rows are the document's text-bearing blocks in reading
//! order, compared with an LCS the same way, so an inserted paragraph shifts
//! nothing and a rewritten one is one changed block, not a delete plus an add.
//! Ids are per parse, so marks always belong to the *newer* document passed in:
//! the export is `(new.id, …)`.

use crate::model::{Block, BlockId, BlockKind, Document};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Only in the newer version.
    Added,
    /// Same kind and position, different text.
    Changed,
}

/// Beyond this many text blocks the table (O(n·m)) is not worth the pause.
const MAX_BLOCKS: usize = 2000;

struct Row {
    id: BlockId,
    /// Blocks of a different kind are never "the same block, edited".
    kind: u8,
    text: String,
}

fn kind_tag(k: &BlockKind) -> u8 {
    match k {
        BlockKind::Text => 0,
        BlockKind::Heading { .. } => 1,
        BlockKind::Bullet | BlockKind::Ordered { .. } | BlockKind::Todo { .. } => 2,
        BlockKind::Code { .. } => 3,
        _ => 4,
    }
}

fn rows(blocks: &[Block], out: &mut Vec<Row>) {
    for b in blocks {
        let text = b.text.plain_text();
        // Containers carry no text of their own; their children do the work.
        if !text.trim().is_empty() {
            out.push(Row { id: b.id, kind: kind_tag(&b.kind), text });
        }
        rows(&b.children, out);
    }
}

pub fn block_changes(old: &Document, new: &Document) -> Vec<(BlockId, Change)> {
    let (mut a, mut b) = (vec![], vec![]);
    rows(&old.blocks, &mut a);
    rows(&new.blocks, &mut b);
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 || n > MAX_BLOCKS || m > MAX_BLOCKS {
        return if n == 0 { b.iter().map(|r| (r.id, Change::Added)).collect() } else { vec![] };
    }
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    let same = |i: usize, j: usize| a[i].kind == b[j].kind && a[i].text == b[j].text;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if same(i, j) {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let mut out = vec![];
    let (mut removed, mut added): (Vec<usize>, Vec<usize>) = (vec![], vec![]);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if same(i, j) {
            flush(&a, &b, &mut removed, &mut added, &mut out);
            i += 1;
            j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            removed.push(i);
            i += 1;
        } else {
            added.push(j);
            j += 1;
        }
    }
    while i < n { removed.push(i); i += 1 }
    while j < m { added.push(j); j += 1 }
    flush(&a, &b, &mut removed, &mut added, &mut out);
    out
}

/// Removed and added runs at the same spot: pair by kind so a rewritten
/// paragraph reads as changed rather than as two unrelated blocks.
fn flush(a: &[Row], b: &[Row], removed: &mut Vec<usize>, added: &mut Vec<usize>, out: &mut Vec<(BlockId, Change)>) {
    for (k, &j) in added.iter().enumerate() {
        let paired = removed.get(k).is_some_and(|&i| a[i].kind == b[j].kind);
        out.push((b[j].id, if paired { Change::Changed } else { Change::Added }));
    }
    removed.clear();
    added.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, BlockKind, RichText};

    fn doc(paras: &[(&str, &str)]) -> Document {
        let mut d = Document::default();
        d.blocks = paras
            .iter()
            .map(|(text, kind)| {
                let k = match *kind {
                    "h" => BlockKind::Heading { level: 1, seq_auto: false },
                    "c" => BlockKind::Code { lang: String::new(), caption: None, wrap: false },
                    _ => BlockKind::Text,
                };
                Block::new(k, RichText::plain(text.to_string()))
            })
            .collect();
        d
    }

    #[test]
    fn identical_documents_change_nothing() {
        let a = doc(&[("one", "p"), ("two", "p")]);
        let b = doc(&[("one", "p"), ("two", "p")]);
        assert!(block_changes(&a, &b).is_empty());
    }

    #[test]
    fn inserted_paragraphs_are_added() {
        let a = doc(&[("one", "p"), ("three", "p")]);
        let b = doc(&[("one", "p"), ("two", "p"), ("three", "p")]);
        let out = block_changes(&a, &b);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, Change::Added);
        assert_eq!(b.blocks[1].id, out[0].0);
    }

    #[test]
    fn rewritten_paragraphs_are_changed_not_replaced() {
        let a = doc(&[("one", "p"), ("two", "p")]);
        let b = doc(&[("one", "p"), ("TWO", "p")]);
        let out = block_changes(&a, &b);
        assert_eq!(out, vec![(b.blocks[1].id, Change::Changed)]);
    }

    #[test]
    fn a_different_kind_is_an_addition() {
        let a = doc(&[("one", "p")]);
        let b = doc(&[("one", "h")]);
        assert_eq!(block_changes(&a, &b), vec![(b.blocks[0].id, Change::Added)]);
    }

    #[test]
    fn nested_blocks_are_compared_too() {
        let mut a = doc(&[("", "p")]);
        a.blocks[0].children = vec![Block::new(BlockKind::Text, RichText::plain("inner"))];
        let mut b = doc(&[("", "p")]);
        b.blocks[0].children = vec![Block::new(BlockKind::Text, RichText::plain("INNER"))];
        let out = block_changes(&a, &b);
        assert_eq!(out, vec![(b.blocks[0].children[0].id, Change::Changed)]);
    }
}
