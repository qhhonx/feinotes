// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Table axes and cells are separate from the legacy block payload. The
//! legacy payload stays as a deterministic starting point for older notes.

use anyhow::{Context, Result, ensure};
use loro::{ExportMode, LoroDoc, LoroMap, LoroMovableList};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use crate::{model::{Block, BlockId, BlockKind, ColorName, RichText, Table, TableAxes, TableCell}, store::blocks::Payload};
use super::content;

type Slot = (u64, u64);

#[derive(Serialize, Deserialize)]
struct Cell {
    bg: Option<String>,
    valign: Option<String>,
    blocks: Vec<CellBlock>,
}

#[derive(Serialize, Deserialize)]
struct CellBlock {
    id: u64,
    payload: Payload,
    children: Vec<CellBlock>,
}

#[derive(Default, Serialize, Deserialize)]
struct Span {
    rows: Vec<u64>,
    cols: Vec<u64>,
}

impl CellBlock {
    fn write(doc: &LoroDoc, block: &Block) -> Result<Self> {
        let mut own = block.without_children();
        own.text = RichText::default();
        content::write(&content::block_text(doc, block.id), &block.text)?;
        let children = block.children.iter().map(|child| Self::write(doc, child)).collect::<Result<_>>()?;
        Ok(Self { id: block.id.0, payload: Payload::encode(&own), children })
    }

    fn read(self, doc: &LoroDoc) -> Result<Block> {
        let id = BlockId(self.id);
        let mut block = self.payload.decode(id)?;
        block.text = content::read(&content::block_text(doc, id))?;
        block.children = self.children.into_iter().map(|child| child.read(doc)).collect::<Result<_>>()?;
        Ok(block)
    }
}

fn axes(list: &LoroMovableList) -> Result<Vec<u64>> {
    (0..list.len()).map(|index| {
        list.get(index).and_then(|value| value.into_value().ok()).and_then(|value| value.as_i64().copied())
            .and_then(|value| u64::try_from(value).ok()).context("Invalid iCloud table axis")
    }).collect()
}

fn order(list: &LoroMovableList, desired: &[u64]) -> Result<()> {
    let mut current = axes(list)?;
    if current == desired { return Ok(()); }
    let wanted: HashSet<_> = desired.iter().copied().collect();
    ensure!(wanted.len() == desired.len(), "Duplicate iCloud table axis");
    let mut seen = HashSet::new();
    let duplicate: Vec<_> = current.iter().map(|id| !seen.insert(*id)).collect();
    for i in (0..current.len()).rev() {
        if !wanted.contains(&current[i]) || duplicate[i] {
            list.delete(i, 1)?;
            current.remove(i);
        }
    }
    let existing: HashSet<_> = current.iter().copied().collect();
    let common: Vec<_> = desired.iter().filter(|id| existing.contains(id)).copied().collect();
    let keep = content::retained_order(&current, &common)?;
    for (index, &id) in desired.iter().enumerate().rev() {
        if keep.contains(&id) { continue; }
        let next = desired.get(index + 1).and_then(|id| current.iter().position(|item| item == id)).unwrap_or(current.len());
        if let Some(from) = current.iter().position(|item| *item == id) {
            let to = next - usize::from(from < next);
            if from != to {
                list.mov(from, to)?;
                current.remove(from);
                current.insert(to, id);
            }
        } else {
            list.insert(next, i64::try_from(id)?)?;
            current.insert(next, id);
        }
    }
    Ok(())
}

fn grid(doc: &LoroDoc, key: &str) -> LoroMap { doc.get_map(format!("{key}-grid")) }
fn owners(doc: &LoroDoc, key: &str) -> LoroMap { doc.get_map(format!("{key}-owners")) }
fn row_list(doc: &LoroDoc, key: &str) -> LoroMovableList { doc.get_movable_list(format!("{key}-rows")) }
fn col_list(doc: &LoroDoc, key: &str) -> LoroMovableList { doc.get_movable_list(format!("{key}-cols")) }
fn slot((r, c): Slot) -> String { format!("{r}-{c}") }

fn key(id: BlockId, legacy: &str) -> String {
    format!("table-{}-{:x}", id.0, Sha256::digest(legacy.as_bytes()))
}

fn identity(bytes: &[u8], flag: u64) -> u64 {
    let digest = Sha256::digest(bytes);
    let bits = u64::from_be_bytes(digest[..8].try_into().unwrap());
    (bits & (flag - 1)) | flag
}

fn with_table(block: &Block) -> Result<Table> {
    let BlockKind::Table(mut table) = block.kind.clone() else { anyhow::bail!("Missing iCloud table") };
    table.init_axes();
    Ok(table)
}

pub(super) fn write(doc: &LoroDoc, meta: &LoroMap, desired: &Block) -> Result<()> {
    let previous = content::value(meta, "shape");
    let base = previous.as_ref().map(|shape| serde_json::from_str::<Payload>(shape)?.decode(desired.id)).transpose()?;
    let (base, shape) = match (base, previous) {
        (Some(base), Some(shape)) if matches!(base.kind, BlockKind::Table(_)) => (base, shape),
        _ => {
            let shape = serde_json::to_string(&Payload::encode(desired))?;
            content::put(meta, "shape", &shape)?;
            (desired.clone(), shape)
        }
    };
    let key = key(desired.id, &shape);
    let wanted = with_table(desired)?;
    if grid(doc, &key).get("shell").is_some() {
        let mut projected = base;
        render(doc, &shape, &mut projected)?;
        let current = with_table(&projected)?;
        if projected == *desired && current.axes.rows == wanted.axes.rows && current.axes.cols == wanted.axes.cols {
            return Ok(());
        }
    } else {
        // Both devices can upgrade the same legacy table offline. Seed its
        // existing cells with the same operations and no device-local parents.
        let seed = content::configured();
        seed.set_peer_id(identity(key.as_bytes(), 1 << 63) & (u64::MAX - 1))?;
        capture(&seed, &key, &base, &with_table(&base)?)?;
        doc.import(&seed.export(ExportMode::updates(&Default::default()))?)?;
    }
    capture(doc, &key, desired, &wanted)
}

fn capture(doc: &LoroDoc, key: &str, block: &Block, table: &Table) -> Result<()> {
    let store = grid(doc, key);
    order(&row_list(doc, key), &table.axes.rows)?;
    order(&col_list(doc, key), &table.axes.cols)?;
    let mut shell = block.without_children();
    shell.kind = BlockKind::Text;
    content::put(&store, "shell", &serde_json::to_string(&Payload::encode(&shell))?)?;
    content::put(&store, "header-rows", &table.header_rows.to_string())?;
    content::put(&store, "header-col", if table.header_col { "1" } else { "0" })?;
    for (&id, width) in table.axes.cols.iter().zip(&table.col_widths) {
        content::put(&store, &format!("width-{id}"), &width.to_string())?;
    }
    let owners = owners(doc, key);
    for (r, c, cell) in table.cells() {
        let cell_key = slot((table.axes.rows[r], table.axes.cols[c]));
        let mut blocks = vec![];
        for block in &cell.blocks {
            content::put(&owners, &block.id.0.to_string(), &cell_key)?;
            blocks.push(CellBlock::write(doc, block)?);
        }
        let shape = Cell { bg: cell.bg.as_ref().map(|color| color.0.clone()), valign: cell.valign.clone(), blocks };
        content::put(&store, &format!("cell-{cell_key}"), &serde_json::to_string(&shape)?)?;
        let span_key = format!("span-{cell_key}");
        if cell.colspan > 1 || cell.rowspan > 1 {
            let span = Span {
                rows: table.axes.rows[r..r + cell.rowspan as usize].to_vec(),
                cols: table.axes.cols[c..c + cell.colspan as usize].to_vec(),
            };
            content::put(&store, &span_key, &serde_json::to_string(&span)?)?;
        } else if store.get(&span_key).is_some() {
            store.delete(&span_key)?;
        }
    }
    Ok(())
}

fn cell(doc: &LoroDoc, grid: &LoroMap, owners: &LoroMap, key: &str, at: Slot) -> Result<TableCell> {
    let slot = slot(at);
    let mut out = TableCell { blocks: vec![], bg: None, valign: None, colspan: 1, rowspan: 1 };
    if let Some(json) = content::value(grid, &format!("cell-{slot}")) {
        let shape: Cell = serde_json::from_str(&json)?;
        out.bg = shape.bg.map(ColorName);
        out.valign = shape.valign;
        for block in shape.blocks {
            if content::value(owners, &block.id.to_string()).as_deref() == Some(&slot) {
                out.blocks.push(block.read(doc)?);
            }
        }
    }
    if out.blocks.is_empty() {
        let mut empty = Block::text("");
        empty.id = BlockId(identity(format!("{key}-{slot}-empty").as_bytes(), 1 << 61));
        out.blocks.push(empty);
    }
    Ok(out)
}

// Reject spans that were separated by a concurrent move or overlap an
// earlier span. In that case the grid keeps its separate cells and text.
fn span_len(ids: &[u64], positions: &HashMap<u64, usize>, first: usize) -> Option<usize> {
    let mut live: Vec<_> = ids.iter().filter_map(|id| positions.get(id).copied()).collect();
    live.sort_unstable();
    (!live.is_empty() && live.iter().copied().eq(first..first + live.len())).then_some(live.len())
}

fn render_spans(grid: &LoroMap, table: &mut Table) -> Result<()> {
    let indices = |ids: &[u64]| ids.iter().enumerate().map(|(index, id)| (*id, index)).collect();
    let rows = indices(&table.axes.rows);
    let cols = indices(&table.axes.cols);
    for r in 0..table.rows.len() {
        for c in 0..table.ncols() {
            if table.rows[r][c].is_covered() { continue; }
            let at = slot((table.axes.rows[r], table.axes.cols[c]));
            let Some(json) = content::value(grid, &format!("span-{at}")) else { continue };
            let span: Span = serde_json::from_str(&json)?;
            let Some(h) = span_len(&span.rows, &rows, r) else { continue };
            let Some(w) = span_len(&span.cols, &cols, c) else { continue };
            if table.rows[r..r + h].iter().any(|row| row[c..c + w].iter().any(TableCell::is_covered)) { continue; }
            let mut extra = vec![];
            for row in r..r + h {
                for col in c..c + w {
                    if (row, col) == (r, c) { continue; }
                    let hidden = std::mem::replace(&mut table.rows[row][col], TableCell::covered());
                    // An empty cell's paragraph may have received text on
                    // another device while the merge was taking place.
                    extra.extend(hidden.blocks.into_iter().filter(|b| !b.text.is_empty() || !b.children.is_empty() || !b.kind.has_text()));
                }
            }
            let origin = &mut table.rows[r][c];
            origin.rowspan = h as u32;
            origin.colspan = w as u32;
            origin.blocks.extend(extra);
        }
    }
    Ok(())
}

pub(super) fn render(doc: &LoroDoc, legacy: &str, block: &mut Block) -> Result<()> {
    let key = key(block.id, legacy);
    let store = grid(doc, &key);
    let Some(shell) = content::value(&store, "shell") else { return Ok(()) };
    let owner = owners(doc, &key);
    let axis = |list: LoroMovableList| -> Result<Vec<u64>> {
        let mut ids = axes(&list)?;
        let mut seen = HashSet::new();
        ids.retain(|id| seen.insert(*id));
        if ids.is_empty() { ids.push(0); }
        Ok(ids)
    };
    let axes = TableAxes { rows: axis(row_list(doc, &key))?, cols: axis(col_list(doc, &key))? };
    let rows = axes.rows.iter().map(|&r| axes.cols.iter().map(|&c| cell(doc, &store, &owner, &key, (r, c)))
        .collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
    let mut table = Table {
        col_widths: axes.cols.iter().map(|id| content::value(&store, &format!("width-{id}"))
            .and_then(|width| width.parse().ok()).unwrap_or(Table::DEFAULT_WIDTH)).collect(),
        header_rows: content::value(&store, "header-rows").and_then(|rows| rows.parse().ok()).unwrap_or(0),
        header_col: content::value(&store, "header-col").as_deref() == Some("1"),
        rows, axes,
    };
    render_spans(&store, &mut table)?;
    table.repair();
    let shell = serde_json::from_str::<Payload>(&shell)?.decode(block.id)?;
    block.align = shell.align;
    block.bg = shell.bg;
    block.folded = shell.folded;
    block.kind = BlockKind::Table(table);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{docxml, model::{CellRange, Document}, store::blocks::assign_seed_ids};

    fn fixture() -> Document {
        let mut table = Table { col_widths: vec![120., 140.], ..Default::default() };
        for labels in [["甲乙", ""], ["丙丁", "戊己"]] {
            table.rows.push(labels.map(|text| TableCell { blocks: vec![Block::text(text)], ..TableCell::empty() }).to_vec());
        }
        let mut doc = Document { blocks: vec![Block::new(BlockKind::Table(table), RichText::default())], ..Default::default() };
        assign_seed_ids(&mut doc);
        doc
    }

    fn table(doc: &mut Document) -> &mut Table {
        let BlockKind::Table(table) = &mut doc.blocks[0].kind else { panic!("Expected table") };
        table
    }

    fn merged(base: &Document, a: &Document, b: &Document) -> Result<Document> {
        let seed = content::seed(base)?;
        let left = content::open(&seed)?;
        let right = content::open(&seed)?;
        let l = content::edit(&left, 101, a)?.context("Missing left edit")?;
        let r = content::edit(&right, 202, b)?.context("Missing right edit")?;
        assert!(left.import(&r)?.pending.is_none());
        assert!(right.import(&l)?.pending.is_none());
        let doc = content::render(&left)?;
        assert_eq!(docxml::serialize(&doc), docxml::serialize(&content::render(&right)?));
        assert!(content::edit(&left, 101, &doc)?.is_none());
        Ok(doc)
    }

    #[test]
    fn offline_legacy_table_upgrades_and_merges_characters_and_cells() -> Result<()> {
        let base = fixture();
        let mut a = base.clone();
        let mut b = base.clone();
        table(&mut a).rows[0][0].blocks[0].text = RichText::plain("左甲乙");
        table(&mut a).rows[1][0].blocks[0].text = RichText::plain("第三格");
        table(&mut b).rows[0][0].blocks[0].text = RichText::plain("甲乙右");
        table(&mut b).rows[1][1].blocks[0].text = RichText::plain("第四格");
        let mut doc = merged(&base, &a, &b)?;
        let t = table(&mut doc);
        assert_eq!(t.rows[0][0].blocks[0].text.text, "左甲乙右");
        assert_eq!(t.rows[1][0].blocks[0].text.text, "第三格");
        assert_eq!(t.rows[1][1].blocks[0].text.text, "第四格");
        Ok(())
    }

    #[test]
    fn concurrent_axis_insert_move_delete_keep_identity_and_text() -> Result<()> {
        let base = fixture();
        let mut a = base.clone();
        let mut b = base.clone();
        let t = table(&mut a);
        t.move_rows(1..2, 0);
        t.insert_col(1);
        t.rows[0][1].blocks[0].text = RichText::plain("新列");
        let t = table(&mut b);
        t.insert_row(1);
        t.delete_cols(1..2);
        t.rows[0][0].blocks[0].text = RichText::plain("改甲乙");
        let mut doc = merged(&base, &a, &b)?;
        let t = table(&mut doc);
        assert_eq!(t.axes.rows.len(), 3);
        assert_eq!(t.axes.cols.len(), 2);
        assert_eq!(t.axes.rows[0], 2);
        assert_eq!(t.rows[0][1].blocks[0].text.text, "新列");
        let old_first = t.axes.rows.iter().position(|id| *id == 1).unwrap();
        assert_eq!(t.rows[old_first][0].blocks[0].text.text, "改甲乙");
        Ok(())
    }

    #[test]
    fn merging_and_splitting_cells_preserve_concurrent_text() -> Result<()> {
        let base = fixture();
        let mut a = base.clone();
        let mut b = base.clone();
        table(&mut a).merge(CellRange { top: 0, bottom: 0, left: 0, right: 1 });
        table(&mut b).rows[0][1].blocks[0].text = RichText::plain("并发填入空格");
        let mut doc = merged(&base, &a, &b)?;
        let t = table(&mut doc);
        assert_eq!(t.rows[0][0].colspan, 2);
        assert_eq!(t.rows[0][0].blocks[1].text.text, "并发填入空格");

        let mut a = doc.clone();
        let mut b = doc.clone();
        table(&mut a).unmerge(0, 0);
        table(&mut b).rows[0][0].blocks[1].text = RichText::plain("保留并发填入空格");
        let mut doc = merged(&doc, &a, &b)?;
        let t = table(&mut doc);
        assert_eq!(t.rows[0][0].colspan, 1);
        assert_eq!(t.rows[0][0].blocks[1].text.text, "保留并发填入空格");
        assert!(t.rows[0][1].blocks[0].text.is_empty());
        Ok(())
    }
}
