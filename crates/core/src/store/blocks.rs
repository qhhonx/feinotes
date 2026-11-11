//! A stored block carries its own content. The document's tree edges are
//! stored separately. Tables retain their cell layout as one block payload.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use crate::{docxml, model::{Block, BlockId, BlockKind, Document, Table}};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payload {
    pub xml: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embedded: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    axes: Vec<crate::model::TableAxes>,
}

fn tables(block: &mut Block, f: &mut impl FnMut(&mut Table)) {
    if let BlockKind::Table(table) = &mut block.kind {
        f(table);
        for cell in table.rows.iter_mut().flatten() {
            for child in &mut cell.blocks { tables(child, f); }
        }
    }
    for child in &mut block.children { tables(child, f); }
}

fn embedded(block: &mut Block, f: &mut impl FnMut(&mut BlockId)) {
    if let BlockKind::Table(table) = &mut block.kind {
        for cell in table.rows.iter_mut().flatten() {
            for child in &mut cell.blocks { walk_ids(child, f); }
        }
    }
}

fn walk_ids(block: &mut Block, f: &mut impl FnMut(&mut BlockId)) {
    f(&mut block.id);
    for child in &mut block.children { walk_ids(child, f); }
    embedded(block, f);
}

pub fn assign_seed_ids(doc: &mut Document) {
    let mut next = 1;
    for block in &mut doc.blocks {
        walk_ids(block, &mut |id| {
            *id = BlockId(next);
            next += 1;
        });
    }
}

impl Payload {
    pub fn encode(block: &Block) -> Self {
        let mut own = block.without_children();
        let mut ids = vec![];
        embedded(&mut own, &mut |id| ids.push(id.0));
        let mut axes = vec![];
        tables(&mut own, &mut |table| axes.push(table.axes.clone()));
        if axes.iter().all(|axes| axes.rows.is_empty() && axes.cols.is_empty()) { axes.clear(); }
        Self { xml: docxml::serialize_block(&own), embedded: ids, axes }
    }

    pub fn decode(&self, id: BlockId) -> Result<Block> {
        let mut block = docxml::parse_single_block(&self.xml)?;
        block.id = id;
        block.children.clear(); // DocxXML supplies a placeholder to empty containers.
        let mut ids = self.embedded.iter();
        let mut valid = true;
        embedded(&mut block, &mut |id| {
            if let Some(value) = ids.next() { *id = BlockId(*value); }
            else { valid = false; }
        });
        ensure!(valid && ids.next().is_none(), "Invalid embedded block identifiers");
        if !self.axes.is_empty() {
            let mut axes = self.axes.iter();
            tables(&mut block, &mut |table| match axes.next() {
                Some(axes) if (axes.rows.is_empty() || axes.rows.len() == table.rows.len())
                    && (axes.cols.is_empty() || axes.cols.len() == table.ncols()) => table.axes = axes.clone(),
                _ => valid = false,
            });
            ensure!(valid && axes.next().is_none(), "Invalid table axis identifiers");
        }
        Ok(block)
    }
}
