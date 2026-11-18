//! Touch entry points around the shared insertion and link commands.
use super::{format_mobile::{SelectionStamp,control,group,submenu}, menus::{self,SlashKind,SlashItem}, view::{Editor,Menu}};
use crate::settings::tr;
use gpui::*;
use serde_json::{Value,json};
use crate::model::{BlockKind,BlockId};

impl Editor {
    fn mobile_attachments(&self) -> Vec<(BlockId,String,Option<std::path::PathBuf>)> {
        let mut items = Vec::new();
        self.core.doc.for_each_block(|block| {
            match &block.kind {
                BlockKind::File(file)=>items.push((block.id,file.name.clone(),super::file::local_path(&self.library_root,file))),
                BlockKind::Image { src,caption,.. }=>{
                    let title = caption.clone().filter(|s|!s.is_empty()).unwrap_or_else(||format!("{} {}",tr("图片","Image"),items.len()+1));
                    items.push((block.id,title,self.img.local(src)));
                }
                _=>{}
            }
        });
        items
    }

    pub(crate) fn present_attachment_controls(&mut self, _cx: &mut Context<Self>) {
        let stamp = SelectionStamp::capture(self,"attachments");
        let mut actions: Vec<_> = self.mobile_attachments().into_iter().map(|(id,title,path)| {
            let mut action = control(&id.0.to_string(),&title,false);
            let available = path.is_some_and(|p|p.is_file());
            action["enabled"] = json!(available);
            if !available { action["detail"] = json!(tr("文件暂不可用","File is unavailable")); }
            action["dismiss"] = json!(true);
            action
        }).collect();
        if actions.is_empty() { let mut action = control("empty",tr("暂无图片或附件","No images or attachments"),false); action["enabled"] = json!(false); actions.push(action); }
        let request = json!({"serial":stamp.serial,"namespace":"attachments","command":"attachment-action","title":tr("图片和附件","Images and attachments"),"done":tr("完成","Done"),
            "context":"","sections":[group("attachments",tr("当前文档","Current document"),false,actions)]});
        self.format_menu = Some(stamp);
        self.table_menu = None;
        self.dismiss_touch_menu();
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_format_controls(json.as_ptr()); } }
    }

    pub(crate) fn mobile_attachment_action(&mut self, serial: u64, name: &str, cx: &mut Context<Self>) {
        let Some(stamp) = self.format_menu.take() else { return };
        if !stamp.matches(self,serial,"attachments") { self.invalidate_table_controls(); return; }
        let target = name.parse::<u64>().ok().and_then(|id|self.mobile_attachments().into_iter().find(|(block,_,_)|block.0==id));
        let Some((_,title,Some(path))) = target.filter(|(_,_,p)|p.as_ref().is_some_and(|p|p.is_file())) else { self.toast(tr("文件不存在","File not found"),cx); return };
        if let (Ok(path),Ok(title)) = (std::ffi::CString::new(path.to_string_lossy().as_bytes()),std::ffi::CString::new(title)) {
            unsafe { fei_ios_preview_named_file(path.as_ptr(),title.as_ptr()); }
        }
    }
}

impl Editor {
    fn mobile_insert_items(&self) -> Vec<(usize,&'static SlashItem)> {
        let mut seen = Vec::new();
        menus::SLASH_ITEMS.iter().enumerate().filter(|(_,item)| {
            if self.markdown && !menus::markdown_ok(item.kind) || seen.contains(&item.kind) { return false; }
            seen.push(item.kind);
            true
        }).collect()
    }

    pub(crate) fn present_insert_controls(&mut self, cx: &mut Context<Self>) {
        self.present_insert_request(false,cx);
    }

    pub(super) fn present_insert_controls_below(&mut self, cx: &mut Context<Self>) {
        self.present_insert_request(true,cx);
    }

    fn present_insert_request(&mut self, below: bool, cx: &mut Context<Self>) {
        if self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { return; }
        self.table_menu = None;
        let stamp = SelectionStamp::capture(self,if below { "insert-below" } else { "insert" });
        let items = self.mobile_insert_items();
        let action = |index,item: &SlashItem| {
            let mut action = control(&insert_key(index,item.kind),crate::settings::text(item.label),false);
            action["dismiss"] = json!(true);
            if item.kind == SlashKind::Link { action["enabled"] = json!(below || self.core.sel.head.block != super::core::TITLE && !self.core.is_code(self.core.sel.head.block)); }
            action
        };
        let quick = [SlashKind::Image,SlashKind::File,SlashKind::Link,SlashKind::Table,SlashKind::Divider];
        let mut shortcuts: Vec<Value> = quick.iter().filter_map(|kind|items.iter().find(|(_,item)|item.kind == *kind)).map(|(index,item)|action(*index,item)).collect();
        let mut photos = control("photos",tr("照片图库","Photo Library"),false);
        photos["dismiss"] = json!(true);
        shortcuts.insert(0,photos);
        let mut sections = vec![group("quick",tr("常用内容","Quick insert"),false,shortcuts)];
        let mut categories = Vec::new();
        for (index,category) in menus::GROUPS.iter().enumerate() {
            let actions: Vec<_> = items.iter().filter(|(_,item)|item.group == index && !quick.contains(&item.kind)).map(|(index,item)|action(*index,item)).collect();
            if actions.is_empty() { continue; }
            let key = format!("insert-group-{index}");
            categories.push(submenu(&key,crate::settings::text(category.title),&key));
            sections.push(group(&key,crate::settings::text(category.title),true,actions));
        }
        sections.insert(1,group("categories",tr("更多内容","More content"),false,categories));
        let request = json!({"serial":stamp.serial,"namespace":"insert","command":"insert-action","title":tr("插入内容","Insert content"),"done":tr("完成","Done"),
            "context":"","sections":sections});
        self.format_menu = Some(stamp);
        self.dismiss_touch_menu();
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_format_controls(json.as_ptr()); } }
    }

    pub(crate) fn mobile_insert_action(&mut self, serial: u64, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stamp) = self.format_menu.take() else { return };
        let below = stamp.matches(self,serial,"insert-below");
        if !(below || stamp.matches(self,serial,"insert")) || self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { self.invalidate_table_controls(); return; }
        let kind = self.mobile_insert_items().into_iter().find(|(index,item)|insert_key(*index,item.kind) == name).map(|(_,item)|item.kind);
        if kind.is_none() && name != "photos" { return; }
        crate::gpui_compat::focus(window,&self.focus,cx);
        if below {
            let Some(after) = self.core.block_sel_ids().last().copied().or_else(||self.mobile_table_block_from_cell()) else { return };
            let paragraph = self.core.insert_paragraph_after(after);
            self.core.clear_block_sel();
            self.set_caret(super::core::Pos::new(paragraph,0),false);
            self.changed(window,cx);
        }
        if name == "photos" { self.pick_photo(window,cx); return; }
        let Some(kind) = kind else { return };
        if kind == SlashKind::Link {
            if self.core.sel.head.block != super::core::TITLE && !self.core.is_code(self.core.sel.head.block) { self.link_from_selection(window,cx); }
            return;
        }
        self.run_slash(kind,window,cx);
        if matches!(kind,SlashKind::Text | SlashKind::Heading(_) | SlashKind::Ordered | SlashKind::Bullet | SlashKind::Todo | SlashKind::Code | SlashKind::Quote | SlashKind::Callout | SlashKind::Grid | SlashKind::Table | SlashKind::Divider) {
            gpui_mobile::show_keyboard();
        }
    }

    pub(super) fn present_native_link(&mut self, only_link: bool, cx: &mut Context<Self>) {
        let stamp = SelectionStamp::capture(self,"link");
        let url = self.link_field.read(cx).text.clone();
        let request = json!({"serial":stamp.serial,"title":tr("链接","Link"),"save":tr("确认","Confirm"),"cancel":tr("取消","Cancel"),
            "text_label":tr("文本","Text"),"url_label":tr("链接地址","URL"),"text":self.link_text_field.read(cx).text,"url":url,
            "text_editable":!only_link,"remove":(!url.is_empty()).then(||tr("移除链接","Remove link"))});
        self.format_menu = Some(stamp);
        self.table_menu = None;
        self.dismiss_touch_menu();
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_link_form(json.as_ptr()); } }
    }

    pub(crate) fn mobile_link_command(&mut self, serial: u64, action: &str, value: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let valid = self.format_menu.as_ref().is_some_and(|stamp| stamp.matches(self,serial,"link"));
        if !valid || (action != "link-cancel" && (self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx))) { self.invalidate_table_controls(); return; }
        let Some(Menu::Link { block,range,.. }) = self.menu.clone() else { self.invalidate_table_controls(); return };
        match action {
            "link-cancel" => self.close_link_panel(window,cx),
            "link-remove" if !self.link_field.read(cx).text.is_empty() => {
                self.close_link_panel(window,cx);
                self.remove_link((block,range),window,cx);
            }
            "link-submit" => {
                #[derive(serde::Deserialize)] struct Fields { text: String, url: String }
                let fields = value.and_then(|v|serde_json::from_str::<Fields>(v).ok());
                let Some(fields) = fields.filter(|f|super::link::is_link_input(f.url.trim())) else {
                    if let Ok(message) = std::ffi::CString::new(tr("请输入有效的链接。","Enter a valid link.")) { unsafe { fei_ios_link_error(serial,message.as_ptr()); } }
                    return;
                };
                self.link_text_field.update(cx,|field,cx|field.set_text(&fields.text,cx));
                self.link_field.update(cx,|field,cx|field.set_text(&fields.url,cx));
                self.submit_link(window,cx);
            }
            _ => self.invalidate_table_controls(),
        }
    }
}

fn insert_key(index: usize, kind: SlashKind) -> String {
    match kind { SlashKind::Image=>"image".into(),SlashKind::File=>"file".into(),SlashKind::Link=>"link".into(),SlashKind::Table=>"table".into(),SlashKind::Divider=>"divider".into(),SlashKind::Board=>"board".into(),_=>format!("slash:{index}") }
}

pub(super) fn dismiss_link_form() { unsafe { fei_ios_dismiss_link_form(); } }
unsafe extern "C" {
    fn fei_ios_present_format_controls(json: *const std::ffi::c_char);
    fn fei_ios_present_link_form(json: *const std::ffi::c_char);
    fn fei_ios_dismiss_link_form();
    fn fei_ios_link_error(serial: u64, message: *const std::ffi::c_char);
    fn fei_ios_preview_named_file(path: *const std::ffi::c_char, title: *const std::ffi::c_char);
}
