// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Keep the original character identities when Enter splits a paragraph or
//! Delete joins two paragraphs. Layout parts point into the existing texts.

use anyhow::{Context, Result, ensure};
use loro::{ContainerTrait, LoroDoc, LoroMap, LoroText, TextDelta, cursor::{Cursor, Side}};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, ops::Range};
use crate::{editor::core::{CloudTransfer, Pos, TITLE}, model::{BlockId, RichText}};
use super::content;

pub(super) const MARK: &str = "quiet-paragraph-boundary";
const BOUNDARY: char = '\u{f0000}';

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Part { root: u64, start: Option<u64> }

fn base_parts(doc: &LoroDoc, block: BlockId) -> Result<Vec<Part>> {
    Ok(content::value(&doc.get_map("paragraph-parts"), &block.0.to_string())
        .map(|value| serde_json::from_str(&value)).transpose()?
        .unwrap_or_else(|| vec![Part { root: block.0, start: None }]))
}

fn joins(doc: &LoroDoc, head: BlockId) -> LoroMap { doc.get_map(format!("paragraph-joins-{}", head.0)) }

fn tails(doc: &LoroDoc, head: BlockId) -> Vec<(i64, BlockId)> {
    let mut tails = vec![];
    joins(doc, head).for_each(|key, value| {
        if let Ok(tail) = key.parse::<u64>()
            && let Some(order) = value.into_value().ok().and_then(|value| value.as_i64().copied())
        { tails.push((order, BlockId(tail))); }
    });
    tails.sort_unstable();
    tails
}

fn parts(doc: &LoroDoc, block: BlockId) -> Result<Vec<Part>> {
    let mut remaining = vec![block];
    let mut seen_blocks = HashSet::new();
    let mut seen_parts = HashSet::new();
    let mut result = vec![];
    while let Some(next) = remaining.pop() {
        if !seen_blocks.insert(next) { continue; }
        result.extend(base_parts(doc, next)?.into_iter().filter(|part| seen_parts.insert(*part)));
        remaining.extend(tails(doc, next).into_iter().rev().map(|(_, id)| id));
    }
    Ok(result)
}

fn set_parts(doc: &LoroDoc, block: BlockId, parts: &[Part]) -> Result<()> {
    content::put(&doc.get_map("paragraph-parts"), &block.0.to_string(), &serde_json::to_string(parts)?)
}

fn span(text: &LoroText, part: Part) -> Option<Range<usize>> {
    let mut at = 0;
    let mut start = part.start.is_none().then_some(0);
    for delta in text.to_delta() {
        let TextDelta::Insert { insert, attributes } = delta else { continue };
        if let Some(id) = attributes.as_ref().and_then(|attrs| attrs.get(MARK))
            .and_then(|value| value.as_string()).and_then(|value| value.parse::<u64>().ok())
        {
            for (index, _) in insert.match_indices(BOUNDARY) {
                if let Some(start) = start { return Some(start..at + index); }
                if Some(id) == part.start { start = Some(at + index + BOUNDARY.len_utf8()); }
            }
        }
        at += insert.len();
    }
    start.map(|start| start..at)
}

fn source(doc: &LoroDoc, part: Part) -> LoroText { content::position_text(doc, BlockId(part.root)) }

pub(super) fn read(doc: &LoroDoc, block: BlockId) -> Result<RichText> {
    if doc.get_map("paragraph-parts").is_empty() { return content::read(&content::position_text(doc, block)); }
    let mut out = RichText::default();
    for part in parts(doc, block)? {
        let text = source(doc, part);
        if let Some(range) = span(&text, part) {
            out.append(&content::read(&text)?.slice(range));
        }
    }
    Ok(out)
}

pub(super) fn transfer(doc: &LoroDoc, transfer: CloudTransfer) -> Result<()> {
    match transfer {
        CloudTransfer::Join { head, tail } => {
            set_parts(doc, head, &base_parts(doc, head)?)?;
            let order = tails(doc, head).last().map_or(0, |(order, _)| order.saturating_add(1));
            joins(doc, head).insert(&tail.0.to_string(), order)?;
        }
        CloudTransfer::Split { at, tail } => {
            let mut left = parts(doc, at.block)?;
            let mut remaining = at.offset;
            for index in 0..left.len() {
                let part = left[index];
                let text = source(doc, part);
                let range = span(&text, part).context("Missing paragraph boundary")?;
                if remaining > range.len() {
                    remaining -= range.len();
                    continue;
                }
                if remaining == range.len() && index + 1 < left.len() {
                    let right = left.split_off(index + 1);
                    joins(doc, at.block).clear()?;
                    set_parts(doc, at.block, &left)?;
                    return set_parts(doc, tail, &right);
                }
                let raw = text.to_string();
                let offset = raw.get(..range.start + remaining).context("Invalid paragraph offset")?.chars().count();
                text.insert(offset, BOUNDARY.encode_utf8(&mut [0; 4]))?;
                text.mark(offset..offset + 1, MARK, tail.0.to_string())?;
                let mut right = left.split_off(index + 1);
                right.insert(0, Part { root: part.root, start: Some(tail.0) });
                joins(doc, at.block).clear()?;
                set_parts(doc, at.block, &left)?;
                return set_parts(doc, tail, &right);
            }
            anyhow::bail!("Could not split paragraph at the editor selection");
        }
    }
    Ok(())
}

fn common_edges(before: &str, after: &str) -> (usize, usize) {
    let prefix = before.chars().zip(after.chars()).take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum::<usize>();
    let suffix = before[prefix..].chars().rev().zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum();
    (prefix, suffix)
}

fn write_part(doc: &LoroDoc, part: Part, desired: &RichText) -> Result<()> {
    let text = source(doc, part);
    let range = span(&text, part).context("Missing paragraph boundary")?;
    let raw = content::read(&text)?;
    if range.start == 0 && range.end == raw.len() { return content::write(&text, desired); }
    let current = raw.slice(range.clone());
    if current == *desired { return Ok(()); }
    let (prefix, suffix) = common_edges(&current.text, &desired.text);
    let start = raw.text[..range.start].chars().count();
    let at = start + current.text[..prefix].chars().count();
    let remove = current.text[prefix..current.len() - suffix].chars().count();
    if remove > 0 { text.delete(at, remove)?; }
    let insert = &desired.text[prefix..desired.len() - suffix];
    if !insert.is_empty() { text.insert(at, insert)?; }
    let current = content::read(&text)?.slice(range.start..range.start + desired.len());
    content::write_marks(&text, start, &current, desired)
}

pub(super) fn write(doc: &LoroDoc, block: BlockId, desired: &RichText) -> Result<()> {
    if doc.get_map("paragraph-parts").is_empty() { return content::write(&content::position_text(doc, block), desired); }
    let parts = parts(doc, block)?;
    if let [part] = parts.as_slice() {
        return write_part(doc, *part, desired);
    }
    let mut before = RichText::default();
    let mut ends = vec![];
    for part in &parts {
        let text = source(doc, *part);
        let range = span(&text, *part).context("Missing paragraph boundary")?;
        before.append(&content::read(&text)?.slice(range));
        ends.push(before.len());
    }
    if before == *desired { return Ok(()); }
    let (prefix, suffix) = common_edges(&before.text, &desired.text);
    let mut start = 0;
    for (index, (part, old_end)) in parts.iter().zip(ends).enumerate() {
        let end = if index + 1 == parts.len() { desired.len() }
            else if old_end <= prefix { old_end }
            else if old_end >= before.len() - suffix { desired.len() - (before.len() - old_end) }
            else { prefix };
        write_part(doc, *part, &desired.slice(start..end))?;
        start = end;
    }
    ensure!(read(doc, block)? == *desired, "Could not apply the edit across paragraph boundaries");
    Ok(())
}

pub(super) fn anchor(doc: &LoroDoc, pos: Pos) -> Result<Option<Cursor>> {
    if doc.get_map("paragraph-parts").is_empty() {
        let text = content::position_text(doc, pos.block);
        return Ok(text.to_string().get(..pos.offset).and_then(|raw| text.get_cursor(raw.chars().count(), Side::Middle)));
    }
    let mut offset = pos.offset;
    for part in parts(doc, pos.block)? {
        let text = source(doc, part);
        let Some(range) = span(&text, part) else { continue };
        if offset <= range.len() {
            return Ok(text.to_string().get(..range.start + offset)
                .and_then(|raw| text.get_cursor(raw.chars().count(), Side::Middle)));
        }
        offset -= range.len();
    }
    Ok(None)
}

fn offset_in(doc: &LoroDoc, block: BlockId, cursor: &Cursor, index: usize) -> Result<Option<Pos>> {
    let mut offset = 0;
    for part in parts(doc, block)? {
        let text = source(doc, part);
        let Some(range) = span(&text, part) else { continue };
        if text.id() == cursor.container {
            let raw = text.to_string();
            let at = raw.char_indices().nth(index).map_or(raw.len(), |(at, _)| at);
            if range.start <= at && at <= range.end {
                return Ok(Some(Pos::new(block, offset + at - range.start)));
            }
        }
        offset += range.len();
    }
    Ok(None)
}

pub(super) fn resolve(doc: &LoroDoc, block: BlockId, cursor: &Cursor) -> Result<Option<Pos>> {
    let Ok(query) = doc.get_cursor_pos(cursor) else { return Ok(None) };
    let index = query.current.pos;
    let direct = offset_in(doc, block, cursor, index)?;
    if doc.get_map("paragraph-parts").is_empty() { return Ok(direct); }
    let mut ids = vec![TITLE];
    content::render(doc)?.for_each_block(|node| ids.push(node.id));
    if direct.is_some() && ids.contains(&block) { return Ok(direct); }
    for id in ids.into_iter().filter(|id| *id != block) {
        if let Some(pos) = offset_in(doc, id, cursor, index)? { return Ok(Some(pos)); }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor::core::{Core, Selection}, model::{Block, Document}};
    use crate::cloud_sync::history::History;
    use loro::ExportMode;

    #[test]
    fn a_split_keeps_remote_characters_and_moves_their_cursor() -> Result<()> {
        let base = Document { title: RichText::plain("拆段"), blocks: vec![Block::text("甲乙丙丁")], ..Default::default() };
        let head = base.blocks[0].id;
        let seed = content::seed(&base)?;
        let mut history = History::new(content::open(&seed)?, 101)?;
        let mut local = Core::new(base.clone());
        let before = history.doc.oplog_vv();
        local.sel = Selection::caret(Pos::new(head, "甲乙".len()));
        local.enter();
        let tail = local.doc.blocks[1].id;
        history.save(&local, 101)?;
        let left = history.doc.export(ExportMode::updates(&before))?;
        let right = content::open(&seed)?;
        let mut remote = Core::new(base);
        remote.sel = Selection::caret(Pos::new(head, "甲乙丙".len()));
        remote.insert_text("远");
        let remote_update = content::edit(&right, 202, &remote.doc)?.unwrap();
        let cursor = content::anchor(&right, remote.sel.head).unwrap();
        history.doc.import(&remote_update)?;
        right.import(&left)?;
        let projected = content::render(&history.doc)?;
        assert_eq!(projected, content::render(&right)?);
        assert_eq!(projected.blocks[0].text.text, "甲乙");
        assert_eq!(projected.blocks[1].text.text, "丙远丁");
        assert_eq!(content::resolve(&right, head, &cursor), Some(Pos::new(tail, "丙远".len())));
        let mut local = Core::new(projected);
        local.sel = Selection::caret(Pos::new(tail, 0));
        local.insert_text("新");
        history.save(&local, 101)?;
        assert_eq!(content::render(&history.doc)?.blocks[1].text.text, "新丙远丁");
        assert!(history.apply(false)?);
        assert!(history.apply(false)?);
        assert_eq!(content::render(&history.doc)?.blocks[0].text.text, "甲乙丙远丁");
        Ok(())
    }

    #[test]
    fn joining_paragraphs_keeps_remote_changes_in_the_removed_block() -> Result<()> {
        let base = Document { title: RichText::plain("并段"), blocks: vec![Block::text("甲乙"), Block::text("丙丁")], ..Default::default() };
        let [head, tail] = [base.blocks[0].id, base.blocks[1].id];
        let seed = content::seed(&base)?;
        let mut history = History::new(content::open(&seed)?, 101)?;
        let mut local = Core::new(base.clone());
        local.sel = Selection::caret(Pos::new(tail, 0));
        local.backspace();
        history.save(&local, 101)?;
        let right = content::open(&seed)?;
        let mut remote = base;
        remote.blocks[1].text = RichText::plain("丙改");
        history.doc.import(&content::edit(&right, 202, &remote)?.unwrap())?;
        let projected = content::render(&history.doc)?;
        assert_eq!(projected.blocks.len(), 1);
        assert_eq!(projected.blocks[0].text.text, "甲乙丙改");
        let mut edit = Core::new(projected);
        edit.sel = Selection::caret(Pos::new(head, "甲乙".len()));
        edit.insert_text("中");
        history.save(&edit, 101)?;
        assert_eq!(content::render(&history.doc)?.blocks[0].text.text, "甲乙中丙改");
        assert!(history.apply(false)? && history.apply(false)?);
        assert_eq!(content::render(&history.doc)?.blocks[1].text.text, "丙改");
        Ok(())
    }

    #[test]
    fn concurrent_joins_of_adjacent_pairs_keep_all_three_paragraphs() -> Result<()> {
        let base = Document { blocks: vec![Block::text("甲"), Block::text("乙"), Block::text("丙")], ..Default::default() };
        let seed = content::seed(&base)?;
        let branch = |peer, tail: BlockId| -> Result<(LoroDoc, Vec<u8>)> {
            let mut history = History::new(content::open(&seed)?, peer)?;
            let before = history.doc.oplog_vv();
            let mut local = Core::new(base.clone());
            local.sel = Selection::caret(Pos::new(tail, 0));
            local.backspace();
            history.save(&local, peer)?;
            let update = history.doc.export(ExportMode::updates(&before))?;
            Ok((history.doc, update))
        };
        let (left, a) = branch(101, base.blocks[1].id)?;
        let (right, b) = branch(202, base.blocks[2].id)?;
        left.import(&b)?;
        right.import(&a)?;
        let doc = content::render(&left)?;
        assert_eq!(doc, content::render(&right)?);
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].text.text, "甲乙丙");
        Ok(())
    }
}
