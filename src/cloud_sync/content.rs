// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Loro owns the ordered block tree and the characters inside each editable block.

use anyhow::{Context, Result, ensure};
use loro::cursor::Cursor;
use loro::{ExportMode, LoroDoc, LoroMap, LoroText, LoroTree, StyleConfig, StyleConfigMap, TextDelta, TreeID, TreeParentId};
use std::collections::{HashMap, HashSet};
use crate::editor::core::{CloudTransfer, Pos, TITLE};
use crate::model::{Block, BlockId, BlockKind, ColorName, Document, Marks, RichText};
use crate::store::blocks::Payload;

const STYLE: &str = "docxxml";
const SEED_PEER: u64 = 1;
const FIELDS: [&str; 9] = ["bold", "italic", "underline", "strike", "code", "link", "color", "background", "inline"];

pub fn local_assets(doc: &Document) -> HashSet<String> {
    doc.asset_sources().into_iter().chain(doc.cover.iter().map(|cover| cover.src.clone()))
        .filter(|src| src.starts_with("assets/") || src.starts_with("file://") || std::path::Path::new(src).is_absolute())
        .collect()
}

pub fn configured() -> LoroDoc {
    let doc = LoroDoc::new();
    let mut styles = StyleConfigMap::new();
    styles.insert(STYLE.into(), StyleConfig::new());
    for field in FIELDS { styles.insert(field.into(), StyleConfig::new()); }
    styles.insert(super::paragraphs::MARK.into(), StyleConfig::new());
    doc.config_text_style(styles);
    doc
}

pub fn open(snapshot: &[u8]) -> Result<LoroDoc> {
    let doc = configured();
    doc.import(snapshot)?;
    Ok(doc)
}

pub fn seed(content: &Document) -> Result<Vec<u8>> {
    let doc = configured();
    doc.set_peer_id(SEED_PEER)?;
    write_document(&doc, content, 0, false)?;
    Ok(doc.export(ExportMode::Snapshot)?)
}

pub fn edit(doc: &LoroDoc, peer: u64, content: &Document) -> Result<Option<Vec<u8>>> {
    edit_with_transfer(doc, peer, content, None)
}

pub fn edit_with_transfer(doc: &LoroDoc, peer: u64, content: &Document, transfer: Option<CloudTransfer>) -> Result<Option<Vec<u8>>> {
    if doc.peer_id() != peer { doc.set_peer_id(peer)?; }
    let before = doc.oplog_vv();
    if let Some(transfer) = transfer { super::paragraphs::transfer(doc, transfer)?; }
    write_document(doc, content, 4, true)?;
    doc.commit();
    if doc.oplog_vv() == before { return Ok(None); }
    Ok(Some(doc.export(ExportMode::updates(&before))?))
}

pub(super) fn block_text(doc: &LoroDoc, id: BlockId) -> LoroText { doc.get_text(format!("block-{}", id.0)) }

pub(super) fn position_text(doc: &LoroDoc, block: BlockId) -> LoroText {
    if block == TITLE { doc.get_text("title") } else { block_text(doc, block) }
}

pub fn anchor(doc: &LoroDoc, pos: Pos) -> Option<Cursor> {
    super::paragraphs::anchor(doc, pos).ok().flatten()
}

pub fn resolve(doc: &LoroDoc, block: BlockId, cursor: &Cursor) -> Option<Pos> {
    super::paragraphs::resolve(doc, block, cursor).ok().flatten()
}

pub(super) fn value(map: &LoroMap, key: &str) -> Option<String> {
    map.get(key).and_then(|value| value.into_value().ok())
        .and_then(|value| value.as_string().map(|text| text.as_str().to_owned()))
}

pub(super) fn put(map: &LoroMap, key: &str, text: &str) -> Result<()> {
    if value(map, key).as_deref() != Some(text) { map.insert(key, text)?; }
    Ok(())
}

struct TreeWrite<'a> {
    doc: &'a LoroDoc,
    tree: LoroTree,
    ids: HashMap<BlockId, TreeID>,
    live: HashSet<TreeID>,
    order: Vec<(Option<TreeID>, Vec<TreeID>)>,
    fine_tables: bool,
}

impl TreeWrite<'_> {
    fn visit(&mut self, blocks: &[Block], parent: Option<TreeID>) -> Result<()> {
        let mut siblings = Vec::with_capacity(blocks.len());
        for block in blocks {
            let id = if let Some(&id) = self.ids.get(&block.id) {
                if self.tree.parent(id) != Some(parent.into()) {
                    self.tree.mov(id, parent)?;
                }
                id
            } else {
                let id = self.tree.create(parent)?;
                self.tree.get_meta(id)?.insert("block", block.id.0.to_string())?;
                self.ids.insert(block.id, id);
                id
            };
            ensure!(self.live.insert(id), "Duplicate block identifier");
            siblings.push(id);
            let mut shape = block.without_children();
            shape.text = RichText::default();
            let meta = self.tree.get_meta(id)?;
            if self.fine_tables && matches!(shape.kind, BlockKind::Table(_)) {
                super::table::write(self.doc, &meta, &shape)?;
            } else {
                put(&meta, "shape", &serde_json::to_string(&Payload::encode(&shape))?)?;
            }
            if self.fine_tables { super::paragraphs::write(self.doc, block.id, &block.text)?; }
            else { write_legacy(&block_text(self.doc, block.id), &block.text)?; }
            self.visit(&block.children, Some(id))?;
        }
        if !siblings.is_empty() { self.order.push((parent, siblings)); }
        Ok(())
    }
}

fn write_document(doc: &LoroDoc, content: &Document, jitter: u8, fine_tables: bool) -> Result<()> {
    let header = Document { title: RichText::default(), icon: content.icon.clone(), cover: content.cover.clone(), blocks: vec![] };
    put(&doc.get_map("note"), "header", &crate::docxml::serialize_header(&header))?;
    if fine_tables { super::paragraphs::write(doc, TITLE, &content.title)?; }
    else { write_legacy(&doc.get_text("title"), &content.title)?; }
    let tree = doc.get_tree("blocks");
    tree.enable_fractional_index(jitter);
    let mut ids = HashMap::new();
    for node in tree.get_nodes(false) {
        let key = block_id(&tree, node.id)?;
        ensure!(ids.insert(key, node.id).is_none(), "Duplicate tree block identifier");
    }
    // Keep removed nodes under a hidden root so undo can reuse their identity.
    let trash = if let Some(id) = ids.remove(&BlockId(0)) { id } else {
        let id = tree.create(None)?;
        tree.get_meta(id)?.insert("block", "0")?;
        id
    };
    let mut writer = TreeWrite { doc, tree, ids, live: HashSet::new(), order: vec![], fine_tables };
    writer.visit(&content.blocks, None)?;
    let tree = writer.tree;
    for node in tree.get_nodes(false) {
        let parent_survives = match node.parent {
            TreeParentId::Node(parent) => writer.live.contains(&parent),
            _ => true,
        };
        if node.id != trash && !writer.live.contains(&node.id) && parent_survives { tree.mov(node.id, trash)?; }
    }
    for (parent, desired) in writer.order {
        let current = tree.children(parent).unwrap_or_default();
        let keep = retained_order(&current, &desired)?;
        for (pos, &id) in desired.iter().enumerate() {
            if keep.contains(&id) { continue; }
            if pos == 0 { tree.mov_to(id, parent, 0)?; }
            else { tree.mov_after(id, desired[pos - 1])?; }
        }
    }
    Ok(())
}

// Only move the complement of the longest still-ordered subsequence.
// Moving unaffected siblings would turn one drag into several concurrent moves.
pub(super) fn retained_order<T: Copy + Eq + std::hash::Hash>(current: &[T], desired: &[T]) -> Result<HashSet<T>> {
    let positions: HashMap<_, _> = current.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut tails: Vec<(usize, usize)> = vec![];
    let mut prev = vec![None; desired.len()];
    for (i, id) in desired.iter().enumerate() {
        let pos = *positions.get(id).context("Missing sibling in Loro tree")?;
        let at = tails.partition_point(|(end, _)| *end < pos);
        if at > 0 { prev[i] = Some(tails[at - 1].1); }
        if at == tails.len() { tails.push((pos, i)); }
        else { tails[at] = (pos, i); }
    }
    let mut keep = HashSet::new();
    let mut index = tails.last().map(|(_, i)| *i);
    while let Some(i) = index { keep.insert(desired[i]); index = prev[i]; }
    Ok(keep)
}

fn block_id(tree: &LoroTree, id: TreeID) -> Result<BlockId> {
    Ok(BlockId(value(&tree.get_meta(id)?, "block").context("Missing block identifier")?.parse()?))
}

pub fn render(doc: &LoroDoc) -> Result<Document> {
    fn read_nodes(doc: &LoroDoc, tree: &LoroTree, nodes: Vec<TreeID>) -> Result<Vec<Block>> {
        let mut out = vec![];
        for node in nodes {
            let id = block_id(tree, node)?;
            if id == BlockId(0) { continue; }
            let meta = tree.get_meta(node)?;
            let shape = value(&meta, "shape").context("Missing block payload")?;
            let mut block = serde_json::from_str::<Payload>(&shape)?.decode(id)?;
            if matches!(block.kind, BlockKind::Table(_)) {
                super::table::render(doc, &shape, &mut block)?;
            }
            block.text = super::paragraphs::read(doc, id)?;
            let mut children = read_nodes(doc, tree, tree.children(node).unwrap_or_default())?;
            let nests = block.kind.nests() || matches!(block.kind, BlockKind::Quote | BlockKind::Callout { .. } | BlockKind::Column { .. });
            if nests {
                block.children = children;
                out.push(block);
            } else if matches!(block.kind, BlockKind::Grid) {
                let mut columns = vec![];
                let mut detached = vec![];
                for child in children {
                    if matches!(child.kind, BlockKind::Column { .. }) { columns.push(child); }
                    else { detached.push(child); }
                }
                if columns.len() > 1 {
                    block.children = columns;
                    out.push(block);
                } else {
                    out.extend(columns.into_iter().flat_map(|column| column.children));
                }
                out.extend(detached);
            } else {
                // A concurrent kind change can leave children below a leaf.
                // Keep their content visible as siblings.
                out.push(block);
                out.append(&mut children);
            }
        }
        Ok(out)
    }
    let header = value(&doc.get_map("note"), "header").context("Missing note header")?;
    let mut target = crate::docxml::parse(&header)?;
    target.title = super::paragraphs::read(doc, TITLE)?;
    let tree = doc.get_tree("blocks");
    target.blocks = read_nodes(doc, &tree, tree.roots())?;
    if target.blocks.is_empty() { target.blocks.push(Block::text("")); }
    Ok(target)
}

fn style(marks: &Marks) -> Option<String> {
    if *marks == Marks::default() { return None; }
    let mut token = RichText::default();
    token.push(if marks.inline.is_some() { "\u{fffc}" } else { "x" }, marks.clone());
    Some(crate::docxml::serialize_inline(&token))
}

fn fields(marks: &Marks) -> [Option<&str>; 9] {
    [
        marks.bold.then_some("1"), marks.italic.then_some("1"), marks.underline.then_some("1"),
        marks.strike.then_some("1"), marks.code.then_some("1"), marks.link.as_deref(),
        marks.text_color.as_ref().map(|color| color.0.as_str()),
        marks.bg_color.as_ref().map(|color| color.0.as_str()),
        marks.inline.as_ref().map(|inline| inline.xml.as_str()),
    ]
}

pub(super) fn read(text: &LoroText) -> Result<RichText> {
    let mut rich = RichText::default();
    for delta in text.to_delta() {
        let TextDelta::Insert { insert, attributes } = delta else { anyhow::bail!("Invalid Loro text") };
        let format = attributes.as_ref().and_then(|attrs| attrs.get(STYLE)).and_then(|value| value.as_string());
        let mut marks = match format {
            Some(xml) => crate::docxml::parse_inline(xml.as_str())?.spans().next()
                .map_or_else(Marks::default, |(_, marks)| marks.clone()),
            None => Marks::default(),
        };
        if let Some(attributes) = attributes {
            for field in FIELDS {
                let Some(raw) = attributes.get(field).and_then(|value| value.as_string()) else { continue };
                // An empty value explicitly clears a mark from the legacy style.
                let value = raw.strip_prefix('+');
                match field {
                    "bold" => marks.bold = value.is_some(),
                    "italic" => marks.italic = value.is_some(),
                    "underline" => marks.underline = value.is_some(),
                    "strike" => marks.strike = value.is_some(),
                    "code" => marks.code = value.is_some(),
                    "link" => marks.link = value.map(str::to_owned),
                    "color" => marks.text_color = value.map(|s| ColorName(s.into())),
                    "background" => marks.bg_color = value.map(|s| ColorName(s.into())),
                    "inline" => marks.inline = if let Some(xml) = value {
                        crate::docxml::parse_inline(xml)?.spans().next().and_then(|(_, marks)| marks.inline.clone())
                    } else { None },
                    _ => unreachable!(),
                }
            }
        }
        // A concurrent formatting op may span an inserted character. Inline
        // objects must still occupy exactly one object-replacement character.
        if marks.inline.is_some() && insert.chars().any(|ch| ch != '\u{fffc}') {
            for ch in insert.chars() {
                let current = if ch == '\u{fffc}' { marks.clone() } else { Marks { inline: None, ..marks.clone() } };
                rich.push(ch.encode_utf8(&mut [0; 4]), current);
            }
        } else {
            rich.push(&insert, marks);
        }
    }
    Ok(rich)
}

pub(super) fn write(text: &LoroText, desired: &RichText) -> Result<()> {
    if &read(text)? == desired { return Ok(()); }
    text.update(&desired.text, Default::default()).map_err(|_| anyhow::anyhow!("Loro text diff timed out"))?;
    let current = read(text)?;
    write_marks(text, 0, &current, desired)?;
    ensure!(&read(text)? == desired, "DocxXML rich text could not be represented exactly");
    Ok(())
}

pub(super) fn write_marks(text: &LoroText, at: usize, current: &RichText, desired: &RichText) -> Result<()> {
    ensure!(current.text == desired.text, "Missing iCloud text");
    let mut existing = current.spans().peekable();
    let mut offset = at;
    for (wanted, marks) in desired.spans() {
        let fields = fields(marks);
        let mut start = wanted.start;
        while start < wanted.end {
            let (range, current) = existing.peek().context("Missing iCloud text range")?;
            let end = range.end.min(wanted.end);
            let next = offset + desired.text[start..end].chars().count();
            for ((key, old), new) in FIELDS.into_iter().zip(self::fields(current)).zip(fields) {
                if old != new {
                    let value = new.map_or_else(String::new, |value| format!("+{value}"));
                    text.mark(offset..next, key, value)?;
                }
            }
            if range.end == end { existing.next(); }
            start = end;
            offset = next;
        }
    }
    Ok(())
}

fn write_legacy(text: &LoroText, desired: &RichText) -> Result<()> {
    if &read(text)? == desired { return Ok(()); }
    text.update(&desired.text, Default::default()).map_err(|_| anyhow::anyhow!("Loro text diff timed out"))?;
    let mut present = Vec::with_capacity(text.len_unicode());
    for delta in text.to_delta() {
        let TextDelta::Insert { insert, attributes } = delta else { anyhow::bail!("Invalid Loro text") };
        let format = attributes.and_then(|mut attrs| attrs.remove(STYLE))
            .and_then(|value| value.as_string().map(|xml| xml.to_string()));
        present.extend(std::iter::repeat_n(format, insert.chars().count()));
    }
    let mut from = 0;
    for (bytes, marks) in desired.spans() {
        let format = style(marks);
        let to = from + desired.text[bytes].chars().count();
        let mut pos = from;
        while pos < to {
            if present[pos] == format { pos += 1; continue; }
            let start = pos;
            while pos < to && present[pos] != format { pos += 1; }
            if let Some(xml) = &format { text.mark(start..pos, STYLE, xml.as_str())?; }
            else { text.unmark(start..pos, STYLE)?; }
        }
        from = to;
    }
    ensure!(&read(text)? == desired, "DocxXML rich text could not be represented exactly");
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::docxml;
    use crate::store::blocks::assign_seed_ids;

    fn fixture(xml: &str) -> Result<Document> {
        let mut doc = docxml::parse(xml)?;
        assign_seed_ids(&mut doc);
        Ok(doc)
    }

    #[test]
    fn independent_seed_replicas_merge_single_character_changes() -> Result<()> {
        let xml = "<title>测试</title><p>海盐<strong>蓝鲸</strong>天空</p>";
        let left = open(&seed(&fixture(xml)?)?)?;
        let right = open(&seed(&fixture(xml)?)?)?;
        let mut a = render(&left)?;
        let mut b = render(&right)?;
        a.blocks[0].text.replace(0..3, &RichText::plain("岩"));
        let from = b.blocks[0].text.text.find('天').unwrap();
        b.blocks[0].text.replace(from..from + 3, &RichText::plain("晴"));
        let a_update = edit(&left, 12, &a)?.context("left update")?;
        let b_update = edit(&right, 19, &b)?.context("right update")?;
        left.import(&b_update)?;
        right.import(&a_update)?;
        let a = render(&left)?;
        assert_eq!(a, render(&right)?);
        assert_eq!(a.blocks[0].text.text, "岩盐蓝鲸晴空");
        assert!(docxml::serialize(&a).contains("<b>蓝鲸</b>"));
        Ok(())
    }

    #[test]
    fn selection_follows_its_text_through_a_remote_insert_and_block_move() -> Result<()> {
        let initial = fixture("<title>计划</title><p>开头🙂保留结尾</p><p>另一段</p>")?;
        let block = initial.blocks[0].id;
        let snapshot = seed(&initial)?;
        let local = open(&snapshot)?;
        let remote = open(&snapshot)?;
        let start = anchor(&local, Pos::new(block, "开头🙂".len())).context("selection start")?;
        let end = anchor(&local, Pos::new(block, "开头🙂保留".len())).context("selection end")?;
        let title = anchor(&local, Pos::new(TITLE, "计".len())).context("title cursor")?;
        let mut updated = initial;
        updated.title.replace(0..0, &RichText::plain("新"));
        updated.blocks[0].text.replace(0..0, &RichText::plain("新增👩‍💻"));
        updated.blocks.swap(0, 1);
        local.import(&edit(&remote, 19, &updated)?.context("remote edit")?)?;
        let start = resolve(&local, block, &start).context("resolved start")?;
        let end = resolve(&local, block, &end).context("resolved end")?;
        assert_eq!(resolve(&local, TITLE, &title), Some(Pos::new(TITLE, "新计".len())));
        let merged = render(&local)?;
        assert_eq!(merged.blocks[1].id, block);
        assert_eq!(&merged.blocks[1].text.text[start.offset..end.offset], "保留");
        Ok(())
    }

    #[test]
    fn cursor_survives_removal_of_its_character() -> Result<()> {
        let initial = fixture("<p>甲🙂乙</p>")?;
        let block = initial.blocks[0].id;
        let snapshot = seed(&initial)?;
        let local = open(&snapshot)?;
        let remote = open(&snapshot)?;
        let cursor = anchor(&local, Pos::new(block, "甲".len())).context("cursor")?;
        let mut updated = initial;
        updated.blocks[0].text.replace("甲".len().."甲🙂".len(), &RichText::default());
        local.import(&edit(&remote, 19, &updated)?.context("remote edit")?)?;
        assert_eq!(resolve(&local, block, &cursor), Some(Pos::new(block, "甲".len())));
        Ok(())
    }

    #[test]
    fn move_insert_delete_and_text_edits_converge() -> Result<()> {
        let initial = fixture("<p>原段落</p><p>删除我</p><blockquote><p>引用</p></blockquote>")?;
        let id = initial.blocks[0].id;
        let state = seed(&initial)?;
        let left = open(&state)?;
        let right = open(&state)?;
        let mut a = initial.clone();
        let moved = a.blocks.remove(0);
        a.blocks.remove(0);
        a.blocks[0].children.push(moved);
        a.blocks.push(Block::text("回车后的新段"));
        let mut b = initial;
        b.blocks[0].text.replace(0..3, &RichText::plain("新"));
        let a_update = edit(&left, 12, &a)?.context("left update")?;
        let b_update = edit(&right, 19, &b)?.context("right update")?;
        left.import(&b_update)?;
        right.import(&a_update)?;
        let doc = render(&left)?;
        assert_eq!(doc, render(&right)?);
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].children[1].id, id);
        assert_eq!(doc.blocks[0].children[1].text.text, "新段落");
        assert_eq!(doc.blocks[1].text.text, "回车后的新段");
        Ok(())
    }

    #[test]
    fn undo_resurrects_same_block_with_its_text() -> Result<()> {
        let initial = fixture("<p>一</p><p>二</p>")?;
        let loro = open(&seed(&initial)?)?;
        let mut removed = initial.clone();
        removed.blocks.remove(0);
        edit(&loro, 12, &removed)?;
        assert_eq!(render(&loro)?, removed);
        edit(&loro, 12, &initial)?;
        assert_eq!(render(&loro)?, initial);
        Ok(())
    }

    #[test]
    fn inline_objects_and_complex_blocks_round_trip() -> Result<()> {
        let xml = include_str!("../../crates/core/tests/fixtures/all-blocks.xml");
        let initial = fixture(xml)?;
        let loro = open(&seed(&initial)?)?;
        assert_eq!(docxml::serialize(&render(&loro)?), docxml::serialize(&initial));
        Ok(())
    }
}
