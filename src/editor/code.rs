//! Shared code-block controls for desktop headers and the native mobile form.
use super::{core,view::Editor};
use crate::model::{BlockId,BlockKind};
use gpui::*;

impl Editor {
    pub(super) fn toggle_code_wrap(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.core.checkpoint(core::EditKind::Other);
        if let Some(b) = self.core.block_mut(id) && let BlockKind::Code { wrap,.. } = &mut b.kind { *wrap = !*wrap; }
        self.changed_preserving_viewport(window,cx);
    }

    pub(super) fn toggle_code_fold(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if !self.code_folded.remove(&id) {
            self.code_folded.insert(id);
            if self.core.sel.head.block == id || self.core.sel.anchor.block == id { self.core.select_block(id); }
        }
        cx.notify();
    }

    pub(super) fn copy_code(&self, id: BlockId, cx: &mut App) -> bool {
        let Some(block) = self.core.block(id).filter(|b|matches!(b.kind,BlockKind::Code { .. })) else { return false };
        cx.write_to_clipboard(ClipboardItem::new_string(block.text.text.clone()));
        true
    }
}

#[cfg(target_os = "ios")]
impl Editor {
    pub(crate) fn has_code_blocks(&self) -> bool {
        let mut found = false;
        self.core.doc.for_each_block(|block| { found |= matches!(block.kind,BlockKind::Code { .. }); });
        found
    }

    pub(super) fn open_native_code(&mut self, block: BlockId, language: bool, cx: &mut Context<Self>) {
        if !matches!(self.core.kind_of(block),Some(BlockKind::Code { .. })) { return; }
        self.lang_field.update(cx,|f,cx|f.set_text("",cx));
        self.menu = Some(super::view::Menu::NativeCode { block,language });
        self.present_native_object(cx);
    }

    pub(super) fn native_code_request(&self, block: BlockId, language: bool, cx: &App) -> Option<serde_json::Value> {
        use super::object_mobile::field;
        use crate::settings::tr;
        use serde_json::json;
        let Some(BlockKind::Code { caption,lang,wrap }) = self.core.kind_of(block) else { return None };
        let editable = !self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx);
        let mut name = field("caption",tr("名称","Name"),"text",caption.as_deref().unwrap_or(crate::settings::text(super::caption::CODE_CAPTION)));
        name["enabled"] = json!(editable);
        let mut languages = field("language",tr("语言","Language"),"choice",lang);
        languages["enabled"] = json!(editable);
        let query = &self.lang_field.read(cx).text;
        languages["options"] = json!(super::menus::filter_languages(query).into_iter().map(|name|json!({"value":name,"title":name})).collect::<Vec<_>>());
        languages["search_key"] = json!("language-search");
        languages["search_text"] = json!(query);
        languages["search_placeholder"] = json!(tr("搜索语言","Search languages"));
        let mut wrapping = field("wrap",tr("自动换行","Wrap lines"),"toggle",if *wrap { "true" } else { "false" });
        wrapping["enabled"] = json!(editable);
        Some(json!({"kind":"code","title":tr("代码块工具","Code tools"),"live":true,"initial":(language && editable).then_some("language"),"fields":[name,languages,wrapping,
            field("folded",tr("折叠代码","Fold code"),"toggle",if self.code_folded.contains(&block) { "true" } else { "false" }),field("copy",tr("复制代码","Copy code"),"action","")]}))
    }

    pub(super) fn mobile_code_event(&mut self, block: BlockId, action: &str, edit: Option<(&str,&str)>, window: &mut Window, cx: &mut Context<Self>) {
        use super::view::Menu;
        match action {
            "object-close" | "object-submit"=>{ self.menu = None; crate::gpui_compat::focus(window,&self.focus,cx); cx.notify(); },
            "object-action" if edit == Some(("copy",""))=>{
                if self.copy_code(block,cx) { self.toast(crate::settings::tr("已复制","Copied"),cx); }
                self.menu = None;
                cx.notify();
            }
            "object-edit"=>match edit {
                Some(("caption",value))=>self.set_caption_text(block,value, cx),
                Some(("language-search",value))=>self.lang_field.update(cx,|f,cx|f.set_text(value,cx)),
                Some(("language",value)) if crate::lang::LANGS.iter().any(|(name,_,_)|*name == value)=>{ self.set_lang(block,value,window,cx); self.menu = Some(Menu::NativeCode { block,language:false }); },
                Some(("wrap",value)) if value == "true" || value == "false"=>if matches!(self.core.kind_of(block),Some(BlockKind::Code { wrap,.. }) if *wrap != (value == "true")) { self.toggle_code_wrap(block,window,cx); },
                Some(("folded",value)) if value == "true" || value == "false"=>if self.code_folded.contains(&block) != (value == "true") { self.toggle_code_fold(block,cx); },
                _=>{},
            },
            _=>{},
        }
    }
}
