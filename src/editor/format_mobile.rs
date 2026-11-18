//! Native format controls use the desktop editor's operations and palettes.
use super::{core::{BlockSel, Selection}, table_ops::CellSel, view::{Editor, Mark}};
use crate::{model::{Align, BlockKind, BlockId, ColorName, Marks}, settings::tr, theme};
use gpui::*;
use serde_json::{Value,json};
use std::sync::atomic::{AtomicU64,Ordering};

pub(super) struct SelectionStamp { pub serial: u64, kind: &'static str, generation: u64, selection: Selection, blocks: Option<BlockSel>, cells: Option<CellSel> }
static SERIAL: AtomicU64 = AtomicU64::new(1);
impl SelectionStamp {
    pub fn capture(ed: &Editor, kind: &'static str) -> Self { Self { serial: SERIAL.fetch_add(1,Ordering::Relaxed),kind,generation:ed.core.generation,selection:ed.core.sel,blocks:ed.core.block_sel,cells:ed.core.cell_sel } }
    pub fn matches(&self, ed: &Editor, serial: u64, kind: &str) -> bool { self.kind == kind && self.serial == serial && self.generation == ed.core.generation && self.selection == ed.core.sel && self.blocks == ed.core.block_sel && self.cells == ed.core.cell_sel }
}

pub(super) fn control(key: &str, title: &str, checked: bool) -> Value { json!({"key":key,"title":title,"enabled":true,"checked":checked,"destructive":false}) }
pub(super) fn submenu(key: &str, title: &str, group: &str) -> Value { let mut action = control(key,title,false); action["submenu"] = json!(group); action }
pub(super) fn group(key: &str, title: &str, hidden: bool, actions: Vec<Value>) -> Value { json!({"key":key,"title":title,"hidden":hidden,"actions":actions}) }

fn color_name(name: Option<&str>) -> &'static str {
    match name {
        None=>tr("默认","Default"),Some("gray")=>tr("灰色","Gray"),Some("red")=>tr("红色","Red"),Some("orange")=>tr("橙色","Orange"),
        Some("yellow")=>tr("黄色","Yellow"),Some("green")=>tr("绿色","Green"),Some("blue")=>tr("蓝色","Blue"),Some("purple")=>tr("紫色","Purple"),
        Some("light-gray")=>tr("浅灰色","Light gray"),Some("light-red")=>tr("浅红色","Light red"),Some("light-orange")=>tr("浅橙色","Light orange"),
        Some("light-yellow")=>tr("浅黄色","Light yellow"),Some("light-green")=>tr("浅绿色","Light green"),Some("light-blue")=>tr("浅蓝色","Light blue"),
        Some("light-purple")=>tr("浅紫色","Light purple"),Some("medium-gray")=>tr("中灰色","Medium gray"),_=>tr("默认","Default"),
    }
}

pub(super) fn palette(prefix: &str, names: &[Option<&str>], current: Option<&str>, fill: fn(&str)->Option<Hsla>) -> Vec<Value> {
    names.iter().map(|name| {
        let mut action = control(&format!("{prefix}{}",name.unwrap_or("none")),color_name(*name),*name == current);
        if let Some(color) = name.and_then(fill) { let color: Rgba = color.into(); action["swatch"] = json!([color.r,color.g,color.b,color.a]); }
        action
    }).collect()
}

fn paragraph_kind(name: &str) -> Option<BlockKind> {
    if let Some(level) = name.strip_prefix("heading").and_then(|v| v.parse::<u8>().ok()).filter(|level| (1..=9).contains(level)) { return Some(super::menus::heading(level)); }
    Some(match name { "text"=>BlockKind::Text,"bullet"=>BlockKind::Bullet { dash: false },"ordered"=>BlockKind::Ordered { seq:None },"todo"=>BlockKind::Todo { done:false },
        "code"=>super::menus::code_block("Plain Text"),"quote"=>BlockKind::Quote,_=>return None })
}

impl Editor {
    pub(super) fn native_format_sections(&self, prefix: &str, nested: bool) -> Vec<Value> {
        let style = |key,title,get: fn(&Marks)->bool| control(&format!("{prefix}{key}"),title,self.mark_active(get));
        let (text,bg) = self.current_colors();
        let mut blocks = vec![control(&format!("{prefix}text"),tr("正文","Text"),self.core.selected_blocks().first().and_then(|id|self.core.kind_of(*id)).is_some_and(|k| matches!(k,BlockKind::Text)))];
        for level in 1..=9 {
            let title = if crate::settings::english() { format!("Heading {level}") } else { format!("{level} 级标题") };
            let checked = self.core.selected_blocks().first().and_then(|id|self.core.kind_of(*id)).is_some_and(|k| matches!(k,BlockKind::Heading { level:l,.. } if *l == level));
            blocks.push(control(&format!("{prefix}heading{level}"),&title,checked));
        }
        for (key,title) in [("bullet",tr("无序列表","Bullet list")),("ordered",tr("有序列表","Numbered list")),("todo",tr("待办列表","Checklist")),("code",tr("代码块","Code block")),("quote",tr("引用","Quote"))] {
            let checked = self.core.selected_blocks().first().and_then(|id|self.core.kind_of(*id)).zip(paragraph_kind(key)).is_some_and(|(a,b)|std::mem::discriminant(a)==std::mem::discriminant(&b));
            blocks.push(control(&format!("{prefix}{key}"),title,checked));
        }
        let fonts: Vec<_> = theme::FONT_COLORS.iter().map(|(name,_)|*name).collect();
        let mut sections = vec![
            group("styles",tr("文字样式","Text style"),nested,vec![style("bold",tr("粗体","Bold"),|m|m.bold),style("italic",tr("斜体","Italic"),|m|m.italic),
                style("underline",tr("下划线","Underline"),|m|m.underline),style("strike",tr("删除线","Strikethrough"),|m|m.strike),style("inline-code",tr("行内代码","Inline code"),|m|m.code),
                submenu("text-colors",tr("字体颜色","Text color"),"text-colors"),submenu("text-highlights",tr("文字背景色","Text highlight"),"text-highlights")]),
            group("paragraph",tr("段落","Paragraph"),nested,vec![
                control(&format!("{prefix}align-left"),tr("左对齐","Align left"),self.current_align()==Align::Left),
                control(&format!("{prefix}align-center"),tr("居中对齐","Align center"),self.current_align()==Align::Center),
                control(&format!("{prefix}align-right"),tr("右对齐","Align right"),self.current_align()==Align::Right),
                submenu("block-types",tr("段落类型","Block type"),"blocks")]),
            group("blocks",tr("段落类型","Block type"),true,blocks),
            group("text-colors",tr("字体颜色","Text color"),true,palette(&format!("{prefix}text-color:"),&fonts,text.as_ref().map(|v|v.0.as_str()),theme::text_color)),
            group("text-highlights",tr("文字背景色","Text highlight"),true,palette(&format!("{prefix}highlight:"),&theme::BG_COLORS,bg.as_ref().map(|v|theme::bg_canon(&v.0)),theme::bg_color)),
        ];
        if !nested && self.core.sel.head.block != super::core::TITLE && !self.core.is_code(self.core.sel.head.block) {
            let mut link = control("link",tr("链接","Link"),false); link["dismiss"] = json!(true);
            sections[0]["actions"].as_array_mut().unwrap().push(link);
        }
        if self.markdown {
            sections[0]["actions"].as_array_mut().unwrap().retain(|a|a.get("submenu").is_none());
            if self.core.cell_sel.is_none() && self.core.cell_of(self.core.sel.head.block).is_none() { sections[1]["actions"].as_array_mut().unwrap().retain(|a|a.get("submenu").is_some()); }
        }
        sections
    }

    pub(crate) fn apply_native_format(&mut self, name: &str, toggle: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { return false; }
        let mark = match name { "bold"=>Some(Mark::Bold),"italic"=>Some(Mark::Italic),"underline"=>Some(Mark::Underline),"strike"=>Some(Mark::Strike),"inline-code"=>Some(Mark::Code),_=>None };
        self.menu = None;
        if let Some(mark) = mark { self.toggle_mark(mark,window,cx); return true; }
        if let Some(align) = match name { "align-left"=>Some(Align::Left),"align-center"=>Some(Align::Center),"align-right"=>Some(Align::Right),_=>None } { if self.markdown && self.core.cell_sel.is_none() && self.core.cell_of(self.core.sel.head.block).is_none() { return false; } self.align(align,window,cx); return true; }
        if let Some(kind) = paragraph_kind(name) { self.turn(kind,toggle,window,cx); return true; }
        for (prefix,names) in [("text-color:",theme::FONT_COLORS.iter().map(|(v,_)|*v).collect::<Vec<_>>()),("highlight:",theme::BG_COLORS.to_vec())] {
            if let Some(value) = name.strip_prefix(prefix) {
                if self.markdown { return false; }
                let color = if value == "none" { None } else { Some(value) };
                if !names.contains(&color) { return false; }
                let color = color.map(|v|ColorName(v.into()));
                if prefix == "text-color:" { self.apply_color(Some(color),None,window,cx); }
                else { self.apply_color(None,Some(color),window,cx); }
                return true;
            }
        }
        false
    }

    pub(crate) fn dismiss_mobile_format(&mut self, cx: &mut Context<Self>) { self.mobile_format = false; self.menu = None; cx.notify(); }

    pub(crate) fn present_format_controls(&mut self, initial: Option<&str>, cx: &mut Context<Self>) {
        if self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { return; }
        self.table_menu = None;
        let stamp = SelectionStamp::capture(self,"format");
        let request = json!({"serial":stamp.serial,"namespace":"format","command":"format-action","title":tr("文字与段落","Text and paragraph"),
            "done":tr("完成","Done"),"context":"","initial":initial.filter(|s|*s == "blocks" || !self.markdown && matches!(*s,"text-colors"|"text-highlights")),"sections":self.native_format_sections("",false)});
        self.format_menu = Some(stamp);
        self.dismiss_touch_menu();
        show_format_request(&request);
    }

    pub(crate) fn mobile_format_action(&mut self, serial: u64, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stamp) = self.format_menu.take() else { return };
        if !stamp.matches(self,serial,"format") || self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { self.invalidate_table_controls(); return; }
        if name == "link" { self.link_from_selection(window,cx); return; }
        if !self.apply_native_format(name,false,window,cx) { self.invalidate_table_controls(); return; }
        self.present_format_controls(None,cx);
    }

    pub(crate) fn present_outline_controls(&mut self, _cx: &mut Context<Self>) {
        self.table_menu = None;
        let stamp = SelectionStamp::capture(self,"outline");
        let mut actions: Vec<_> = super::catalogue::entries(&self.core.doc).into_iter().map(|entry| {
            let mut item = control(&entry.id.0.to_string(),&entry.text,Some(entry.id) == self.cat_active);
            item["depth"] = json!(entry.level.saturating_sub(1));
            item
        }).collect();
        if actions.is_empty() { actions.push(control(&super::core::TITLE.0.to_string(),&crate::settings::display_title(&self.core.doc.title.text),true)); }
        let request = json!({"serial":stamp.serial,"namespace":"outline","command":"outline-action","title":tr("文档目录","Outline"),"done":tr("完成","Done"),
            "context":"","sections":[group("outline",tr("标题","Headings"),false,actions)]});
        self.format_menu = Some(stamp);
        self.dismiss_touch_menu();
        show_format_request(&request);
    }

    pub(crate) fn mobile_outline_action(&mut self, serial: u64, name: &str, cx: &mut Context<Self>) {
        let Some(stamp) = self.format_menu.take() else { return };
        let id = name.parse::<u64>().ok().map(BlockId);
        let Some(id) = id.filter(|id| *id == super::core::TITLE || super::catalogue::entries(&self.core.doc).iter().any(|entry|entry.id == *id)) else { self.invalidate_table_controls(); return };
        if !stamp.matches(self,serial,"outline") { self.invalidate_table_controls(); return; }
        self.invalidate_table_controls();
        self.jump_to_heading(id,cx);
    }
}

pub(super) fn show_format_request(request: &Value) {
    if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_format_controls(json.as_ptr()); } }
}

unsafe extern "C" { fn fei_ios_present_format_controls(json: *const std::ffi::c_char); }
