//! Native forms adapt the shared inline-object editors without changing persistence.
use super::{core::Pos,format_mobile::{SelectionStamp,control,group},view::{Editor,Menu}};
use crate::{model::{BlockId,BlockKind,InlineKind},settings::tr};
use gpui::*;
use serde_json::{Value,json};

pub(super) fn field(key: &str, title: &str, kind: &str, value: &str) -> Value { json!({"key":key,"title":title,"kind":kind,"value":value}) }
pub(super) fn date_millis(value: &str) -> Option<i64> { value.parse().ok().filter(|v|(-62_135_596_800_000..=253_402_300_799_999).contains(v)) }
pub(super) fn date_field(key: &str, title: &str, value: i64, whole: bool) -> Value {
    let offset = crate::time::local_offset();
    let title = format!("{title} (GMT{}{hours:02}:{minutes:02})",if offset < 0 { "-" } else { "+" },hours=offset.abs()/3600,minutes=offset.abs()%3600/60);
    let mut field = field(key,&title,"date",&value.to_string());
    field["offset"] = json!(offset);
    field["date_mode"] = json!(if whole { "date" } else { "dateAndTime" });
    field
}
pub(super) fn dismiss_form() { unsafe { fei_ios_dismiss_object_form(); } }

impl Editor {
    fn mobile_objects(&self, editable: bool) -> Vec<(String,String)> {
        let mut objects = Vec::new();
        let mut board_number = 0;
        self.core.doc.for_each_block(|block| {
            if let BlockKind::Code { caption,lang,.. } = &block.kind { objects.push((format!("code:{}:0",block.id.0),format!("{}：{} ({lang})",tr("代码块","Code block"),caption.as_deref().unwrap_or(crate::settings::text(super::caption::CODE_CAPTION))))); }
            if !editable { return; }
            if let BlockKind::Image { caption,.. } = &block.kind { objects.push((format!("image:{}:0",block.id.0),format!("{}：{}",tr("图片","Image"),caption.as_deref().unwrap_or("")))); }
            if let BlockKind::File(file) = &block.kind && self.file_preview_frame(block.id).is_some() {
                objects.push((format!("file-preview:{}:0",block.id.0),format!("{}：{}",tr("附件预览尺寸","Attachment preview size"),file.name)));
            }
            if matches!(block.kind,BlockKind::Countdown(_)) { objects.push((format!("countdown:{}:0",block.id.0),tr("倒计时","Countdown").into())); }
            if matches!(block.kind,BlockKind::Board(_)) {
                board_number += 1;
                objects.push((format!("board:{}:0",block.id.0),format!("{} {board_number}",tr("编辑画板","Edit board"))));
            }
            for (range,marks) in block.text.spans() {
                let Some(inline) = &marks.inline else { continue };
                let (kind,title,detail) = match &inline.kind {
                    InlineKind::Equation(source)=>("equation",tr("公式","Equation"),source.clone()),
                    InlineKind::Button(button)=>("button",tr("按钮","Button"),button.text.clone()),
                    InlineKind::Reminder(reminder)=>("reminder",tr("日期提醒","Date reminder"),super::reminder::reminder_label(reminder," ")),
                    _=>continue,
                };
                for offset in (range.start..range.end).step_by(super::core::EQUATION.len()) {
                    let detail: String = detail.chars().take(60).collect();
                    objects.push((format!("{kind}:{}:{offset}",block.id.0),if detail.is_empty() { title.into() } else { format!("{title}：{detail}") }));
                }
            }
        });
        objects
    }

    pub(crate) fn present_object_list(&mut self, cx: &mut Context<Self>) {
        let editable = !self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx);
        let stamp = SelectionStamp::capture(self,"object-list");
        let mut actions: Vec<_> = self.mobile_objects(editable).into_iter().map(|(key,title)| { let mut a = control(&key,&title,false); a["dismiss"] = json!(true); a }).collect();
        if actions.is_empty() { let mut a = control("empty",tr("暂无可设置内容","No configurable content"),false); a["enabled"] = json!(false); actions.push(a); }
        let request = json!({"serial":stamp.serial,"namespace":"objects","command":"object-open","title":if editable { tr("内容设置","Content settings") } else { tr("代码块工具","Code tools") },"done":tr("完成","Done"),"context":"","sections":[group("objects",tr("当前文档","Current document"),false,actions)]});
        self.table_menu = None;
        self.format_menu = Some(stamp);
        self.dismiss_touch_menu();
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_format_controls(json.as_ptr()); } }
    }

    pub(crate) fn mobile_object_open(&mut self, serial: u64, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stamp) = self.format_menu.as_ref() else { return };
        if stamp.serial != serial { return; }
        if !stamp.matches(self,serial,"object-list") { self.invalidate_table_controls(); return; }
        if !self.mobile_objects(!self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx)).iter().any(|(key,_)|key == name) { return; }
        let mut parts = name.split(':');
        let (Some(kind),Some(block),Some(offset)) = (parts.next(),parts.next().and_then(|v|v.parse::<u64>().ok()).map(BlockId),parts.next().and_then(|v|v.parse::<usize>().ok())) else { return };
        self.format_menu = None;
        match kind {
            "code"=>self.open_native_code(block,false,cx),
            "image"=>self.open_native_image(block,window,cx),
            "file-preview"=>self.open_native_file_preview(block,window,cx),
            "equation"=>self.open_equation_editor(Pos::new(block,offset),window,cx),
            "button"=>self.open_button_panel(Pos::new(block,offset),window,cx),
            "reminder"=>self.open_reminder(Pos::new(block,offset),cx),
            "countdown"=>self.open_countdown(block,window,cx),
            "board"=>self.open_board(block,window,cx),
            _=>{},
        }
    }

    pub(super) fn present_native_object(&mut self, cx: &mut Context<Self>) {
        if (self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx)) && !matches!(self.menu,Some(Menu::NativeCode { .. })) { return; }
        let stamp = SelectionStamp::capture(self,"object-form");
        let serial = stamp.serial;
        self.table_menu = None;
        self.format_menu = Some(stamp);
        self.dismiss_touch_menu();
        self.publish_native_object(serial,cx);
    }

    pub(super) fn refresh_native_image_form(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.menu,Some(Menu::NativeImage { .. })) { return; }
        if let Some(serial) = self.format_menu.as_ref().filter(|stamp|stamp.matches(self,stamp.serial,"object-form")).map(|stamp|stamp.serial) { self.publish_native_object(serial,cx); }
    }

    fn publish_native_object(&mut self, serial: u64, cx: &App) {
        let request = match self.menu {
            Some(Menu::NativeImage { block })=>self.native_image_request(block),
            Some(Menu::NativeFilePreview { block })=>self.native_file_preview_request(block),
            Some(Menu::NativeCode { block,language })=>self.native_code_request(block,language,cx),
            Some(Menu::Equation { block,offset })=>self.core.equation_source(Pos::new(block,offset)).map(|source|json!({"kind":"equation","title":tr("公式","Equation"),"live":true,"fields":[field("source","LaTeX","multiline",&source)]})),
            Some(Menu::Button { .. })=>self.native_button_request(cx),
            Some(Menu::Reminder { .. })=>self.native_reminder_request(),
            _ if self.cd.dialog.is_some()=>self.native_countdown_request(cx),
            _=>None,
        };
        let Some(mut request) = request else { self.format_menu = None; dismiss_form(); return };
        request["serial"] = json!(serial);
        request["save"] = json!(tr("确定","Confirm"));
        request["cancel"] = json!(tr("取消","Cancel"));
        request["done"] = json!(tr("完成","Done"));
        let mut stamp = SelectionStamp::capture(self,"object-form");
        stamp.serial = serial;
        self.format_menu = Some(stamp);
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_object_form(json.as_ptr()); } }
    }

    pub(crate) fn mobile_object_command(&mut self, serial: u64, action: &str, value: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stamp) = self.format_menu.as_ref() else { return };
        if stamp.serial != serial { return; }
        #[derive(serde::Deserialize)] struct Edit { key: String, value: String }
        let edit = value.and_then(|value|serde_json::from_str::<Edit>(value).ok());
        let reading_action = matches!(self.menu,Some(Menu::NativeCode { .. })) && match action {
            "object-close" | "object-submit"=>true,
            "object-edit"=>edit.as_ref().is_some_and(|e|matches!(e.key.as_str(),"folded"|"language-search")),
            "object-action"=>edit.as_ref().is_some_and(|e|e.key == "copy" && e.value.is_empty()),
            _=>false,
        };
        if !stamp.matches(self,serial,"object-form") || (!reading_action && (self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx))) {
            if matches!(self.menu,Some(Menu::NativeImage { .. })) { self.cancel_crop(cx); self.end_resize(window,cx); }
            if matches!(self.menu,Some(Menu::NativeFilePreview { .. })) { self.cancel_file_resize(); }
            self.menu = None;
            self.cd.dialog = None;
            self.invalidate_table_controls();
            self.toast(tr("文档已更新，请重新打开设置。","The document changed. Please reopen its settings."),cx);
            return;
        }
        match self.menu {
            Some(Menu::NativeImage { block })=>self.mobile_image_event(block,action,edit.as_ref().map(|e|(e.key.as_str(),e.value.as_str())),window,cx),
            Some(Menu::NativeFilePreview { block })=>self.mobile_file_preview_event(block,action,edit.as_ref().map(|e|(e.key.as_str(),e.value.as_str())),window,cx),
            Some(Menu::NativeCode { block,.. })=>self.mobile_code_event(block,action,edit.as_ref().map(|e|(e.key.as_str(),e.value.as_str())),window,cx),
            Some(Menu::Equation { .. })=>match action {
                "object-edit"=>if let Some(edit) = edit.filter(|e|e.key == "source") { self.eq_field.update(cx,|field,cx|field.set_text(&edit.value,cx)); self.update_equation_source(&edit.value,window,cx); },
                "object-submit" | "object-close"=>self.close_equation_editor(true,window,cx),
                _=>return,
            },
            Some(Menu::Button { .. })=>self.mobile_button_event(action,edit.as_ref().map(|e|(e.key.as_str(),e.value.as_str())),window,cx),
            Some(Menu::Reminder { .. })=>self.mobile_reminder_event(action,edit.as_ref().map(|e|(e.key.as_str(),e.value.as_str())),window,cx),
            _ if self.cd.dialog.is_some()=>self.mobile_countdown_event(action,edit.as_ref().map(|e|(e.key.as_str(),e.value.as_str())),window,cx),
            _=>return,
        }
        self.publish_native_object(serial,cx);
    }
}

unsafe extern "C" {
    fn fei_ios_present_format_controls(json: *const std::ffi::c_char);
    fn fei_ios_present_object_form(json: *const std::ffi::c_char);
    fn fei_ios_dismiss_object_form();
}


pub(super) fn alignment_field(align: crate::model::Align) -> Value {
    use crate::model::Align;
    use crate::settings::tr;
        let mut alignment = field("alignment",tr("对齐","Alignment"),"choice",match align { Align::Left=>"left",Align::Center=>"center",Align::Right=>"right" });
        alignment["options"] = json!([{"value":"left","title":tr("左对齐","Align left")},{"value":"center","title":tr("居中对齐","Align center")},{"value":"right","title":tr("右对齐","Align right")}]);
    alignment
}

pub(super) fn alignment_value(value: &str) -> Option<crate::model::Align> {
    use crate::model::Align;
    match value {
        "left" => Some(Align::Left),
        "center" => Some(Align::Center),
        "right" => Some(Align::Right),
        _ => None,
    }
}
