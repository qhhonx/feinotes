// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use anyhow::Result;
use loro::{LoroDoc, UndoManager};
use crate::editor::core::Core;
use super::content;

pub(super) struct History {
    pub doc: LoroDoc,
    undo: UndoManager,
    group: Option<u64>,
}

impl History {
    pub fn new(doc: LoroDoc, peer: u64) -> Result<Self> {
        doc.set_peer_id(peer)?;
        let mut undo = UndoManager::new(&doc);
        undo.add_exclude_origin_prefix("sync-metadata");
        undo.set_max_undo_steps(500);
        Ok(Self { doc, undo, group: None })
    }

    pub fn save(&mut self, core: &Core, peer: u64) -> Result<()> {
        for step in core.cloud_save_steps(self.group) {
            let group = step.group;
            if self.group != group || group.is_none() {
                self.undo.group_end();
                self.undo.group_start()?;
            }
            let transfer = (self.group != group).then_some(step.transfer).flatten();
            content::edit_with_transfer(&self.doc, peer, step.body, transfer)?;
            self.group = group;
        }
        Ok(())
    }

    pub fn apply(&mut self, redo: bool) -> Result<bool> {
        self.undo.group_end();
        self.group = None;
        while if redo { self.undo.can_redo() } else { self.undo.can_undo() } {
            if if redo { self.undo.redo()? } else { self.undo.undo()? } {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor::core::{EditKind, Pos, Selection}, model::{Block, Document, Marks, RichText}};

    #[test]
    fn undo_redo_keeps_remote_text_and_independent_style() -> Result<()> {
        let base = Document { blocks: vec![Block::text("甲乙")], ..Default::default() };
        let seed = content::seed(&base)?;
        let mut history = History::new(content::open(&seed)?, 101)?;
        let mut editor = Core::new(base.clone());
        let id = base.blocks[0].id;
        editor.sel = Selection::caret(Pos::new(id, 0));
        editor.insert_text("左");
        history.save(&editor, 101)?;
        editor.insert_text("边");
        editor.checkpoint(EditKind::Other);
        let mut bold = RichText::plain("左边");
        bold.push("甲乙", Marks { bold: true, ..Default::default() });
        editor.doc.blocks[0].text = bold;
        history.save(&editor, 101)?;

        let remote = content::open(&seed)?;
        let mut edited = base.clone();
        let mut italic = RichText::default();
        italic.push("甲乙", Marks { italic: true, ..Default::default() });
        italic.push("右", Marks::default());
        edited.blocks[0].text = italic;
        history.doc.import(&content::edit(&remote, 202, &edited)?.unwrap())?;
        let before_undo = content::render(&history.doc)?;
        assert_eq!(before_undo.blocks[0].text.text, "左边甲乙右");
        let combined = before_undo.blocks[0].text.marks_at("左边甲".len());
        assert!(combined.bold && combined.italic);

        assert!(history.apply(false)?);
        let style_undone = content::render(&history.doc)?;
        assert_eq!(style_undone.blocks[0].text.text, "左边甲乙右");
        let marks = style_undone.blocks[0].text.marks_at("左边甲".len());
        assert!(!marks.bold && marks.italic);

        assert!(history.apply(false)?);
        assert_eq!(content::render(&history.doc)?.blocks[0].text.text, "甲乙右");
        assert!(history.apply(true)? && history.apply(true)?);
        assert_eq!(content::render(&history.doc)?.blocks[0].text, before_undo.blocks[0].text);
        Ok(())
    }

    #[test]
    fn undo_move_and_delete_restore_blocks_with_remote_edits() -> Result<()> {
        let base = Document { blocks: vec![Block::text("甲"), Block::text("乙"), Block::text("丙")], ..Default::default() };
        let seed = content::seed(&base)?;
        let mut history = History::new(content::open(&seed)?, 101)?;
        let mut editor = Core::new(base.clone());
        editor.move_block(base.blocks[1].id, base.blocks[2].id, true);
        editor.checkpoint(EditKind::Other);
        editor.doc.blocks.remove(0);
        history.save(&editor, 101)?;
        let remote = content::open(&seed)?;
        let mut edited = base.clone();
        edited.blocks[0].text = RichText::plain("改甲");
        history.doc.import(&content::edit(&remote, 202, &edited)?.unwrap())?;
        let visible = |doc: &LoroDoc| -> Result<Vec<String>> {
            Ok(content::render(doc)?.blocks.into_iter().map(|block| block.text.text).collect())
        };
        assert_eq!(visible(&history.doc)?, ["丙", "乙"]);
        assert!(history.apply(false)?);
        assert_eq!(visible(&history.doc)?, ["改甲", "丙", "乙"]);
        assert!(history.apply(false)?);
        assert_eq!(visible(&history.doc)?, ["改甲", "乙", "丙"]);
        Ok(())
    }
}
