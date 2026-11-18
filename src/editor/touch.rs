//! Touch selection around the shared Core and the system edit menu.
use super::{core::{Pos, Selection}, view::Editor};
use crate::editor_theme::Theme;
use gpui::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct TouchState { block_drag: bool, drag: Option<Drag>, menu: Option<MenuStamp>, point: Point<Pixels> }
struct MenuStamp { serial: u64, generation: u64, selection: Selection, blocks: Option<super::core::BlockSel>, cells: Option<super::table_ops::CellSel> }
#[derive(Clone,Copy)]
enum Drag {
    Word { before: Selection, word: Selection, start: Point<Pixels> },
    Handle { before: Selection, fixed: Pos, offset: Point<Pixels> },
}
static SERIAL: AtomicU64 = AtomicU64::new(1);

impl Editor {
    pub(super) fn reset_touch_selection(&mut self) {
        self.touch.drag = None;
        if std::mem::take(&mut self.touch.block_drag) { self.block_drag = None; self.toolbar_hidden = false; }
        self.dragging = false;
        self.dismiss_touch_menu();
        self.invalidate_table_controls();
    }

    pub(super) fn dismiss_touch_menu(&mut self) {
        self.touch.menu = None;
        unsafe { fei_ios_dismiss_text_menu(); }
    }

    pub(super) fn present_touch_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let serial = SERIAL.fetch_add(1,Ordering::Relaxed);
        self.touch.point = position;
        self.touch.menu = Some(MenuStamp { serial,generation: self.core.generation,selection: self.core.sel,
            blocks: self.core.block_sel,cells: self.core.cell_sel });
        let (start,end) = self.core.ordered();
        let rect = self.point_of(start).zip(self.point_of(end)).map(|((a,ah),(b,bh))| {
            [f32::from(a.x.min(b.x)-px(10.)),f32::from(a.y.min(b.y)-px(12.)),
                f32::from((a.x-b.x).abs()+px(20.)),f32::from((a.y-b.y).abs()+ah.max(bh)+px(24.))]
        }).unwrap_or([f32::from(position.x),f32::from(position.y),1.,20.]);
        let handles: Vec<[f32;2]> = self.touch_handles().into_iter().map(|(first,p,h)|
            [f32::from(p.x),f32::from(p.y+if first { -px(5.) } else { h+px(5.) })]).collect();
        let tr = crate::settings::tr;
        let request = serde_json::json!({ "serial": serial,"x": f32::from(position.x),"y": f32::from(position.y),"rect": rect,"handles": handles,
            "editable": !self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx),"selected": !self.core.sel.is_collapsed() || self.core.in_block_mode() || self.core.cell_sel.is_some(),
            "labels": { "cut": tr("剪切","Cut"),"copy": tr("复制","Copy"),"paste": tr("粘贴","Paste"),
                "select-all": tr("全选","Select All"),"bold": tr("加粗","Bold"),"italic": tr("斜体","Italic"),"format":tr("更多样式…","More styles…") } });
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_text_menu(json.as_ptr()); } }
    }

    pub(crate) fn validate_touch_menu(&mut self, serial: u64, action: &str, cx: &App) -> bool {
        let Some(stamp) = self.touch.menu.take() else { return false };
        stamp.serial == serial && stamp.generation == self.core.generation && stamp.selection == self.core.sel && stamp.blocks == self.core.block_sel && stamp.cells == self.core.cell_sel
            && (matches!(action,"copy" | "select-all") || (!self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx)))
    }

    pub(crate) fn repeat_touch_menu(&mut self, cx: &mut Context<Self>) { self.present_touch_menu(self.touch.point,cx); }

    fn touch_long_press(&mut self, event: &LongPressEvent, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        match event.phase {
            TouchPhase::Started => {
                if window.default_prevented() || !bounds.contains(&event.start_position) { return; }
                if let Some(id) = self.touch_block_handle_at(event.start_position,window,cx) {
                    self.dismiss_touch_menu();
                    self.press_handle(id,event.start_position);
                    self.touch.block_drag = true;
                    window.capture_long_press(&cx.entity());
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if self.menu.is_some() || self.slash.is_some() || self.pos_at(event.start_position).is_none() { return; }
                let before = self.core.sel;
                self.link_ui.click = None;
                self.dismiss_touch_menu();
                let Some(pos) = self.pos_at(event.start_position) else { return };
                let Some(text) = self.core.text_of(pos.block) else { return };
                let word = super::core::word_at(&text.text,pos.offset);
                crate::gpui_compat::focus(window,&self.focus,cx);
                self.core.clear_block_sel();
                self.set_caret(Pos::new(pos.block,word.start),false);
                self.core.sel.head = Pos::new(pos.block,word.end);
                self.dragging = false;
                self.touch.drag = Some(Drag::Word { before,word: self.core.sel,start: event.start_position });
                self.toolbar_hidden = true;
                window.capture_long_press(&cx.entity());
                window.prevent_default();
                cx.stop_propagation();
            }
            TouchPhase::Moved if window.has_long_press_capture(&cx.entity()) => {
                if self.touch.block_drag {
                    self.block_drag_move(event.position,true,window,cx);
                    cx.stop_propagation();
                    return;
                }
                if let Some(Drag::Word { word,start,.. }) = self.touch.drag {
                    if (event.position-start).magnitude() > 5. {
                        if let Some(pos) = self.pos_at(event.position) {
                            let units = self.core.units();
                            let order = |p: Pos| (units.iter().position(|unit| unit.id == p.block).unwrap_or(0),p.offset);
                            let fixed = if order(pos) < order(word.anchor) { word.head } else { word.anchor };
                            if order(pos) < order(word.anchor) || order(pos) > order(word.head) { self.touch_extend(fixed,pos,cx); }
                        }
                    }
                }
                cx.stop_propagation();
            }
            TouchPhase::Ended if window.has_long_press_capture(&cx.entity()) && self.touch.block_drag => {
                self.touch.block_drag = false;
                self.block_drag_move(event.position,true,window,cx);
                self.end_block_drag(Some(event.position),window,cx);
                cx.stop_propagation();
            }
            TouchPhase::Ended if window.has_long_press_capture(&cx.entity()) && matches!(self.touch.drag,Some(Drag::Word { .. })) => {
                self.touch.drag = None;
                self.dragging = false;
                self.present_touch_menu(event.position,cx);
                cx.notify();
                cx.stop_propagation();
            }
            TouchPhase::Cancelled => {
                if std::mem::take(&mut self.touch.block_drag) { self.cancel_block_drag(cx); }
                self.cancel_touch_drag(cx);
            }
            _ => {}
        }
    }

    fn touch_extend(&mut self, fixed: Pos, pos: Pos, cx: &mut Context<Self>) {
        if self.core.cell_of(fixed.block) != self.core.cell_of(pos.block) { return; }
        self.core.sel = Selection { anchor: fixed,head: pos };
        self.core.pending_marks = None;
        self.toolbar_hidden = true;
        cx.notify();
    }

    fn cancel_touch_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.touch.drag.take() {
            self.core.sel = match drag { Drag::Word { before,.. } | Drag::Handle { before,.. } => before };
            self.dragging = false;
            self.dismiss_touch_menu();
            cx.notify();
        }
    }

    fn touch_handles(&self) -> Vec<(bool,Point<Pixels>,Pixels)> {
        if self.core.sel.is_collapsed() || self.core.in_block_mode() || self.core.cell_sel.is_some() { return vec![]; }
        let (start,end) = self.core.ordered();
        [(true,start),(false,end)].into_iter().filter_map(|(first,pos)| self.point_of(pos).map(|(p,h)| (first,p,h))).collect()
    }

    fn touch_handle_drag(&mut self, event: &TouchDragEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.phase {
            TouchPhase::Started if !window.default_prevented() => {
                let candidate = self.touch_handles().into_iter().map(|(first,p,h)| {
                    let knob = point(p.x,p.y+if first { -px(5.) } else { h+px(5.) });
                    (first,p,h,(knob-event.start_position).magnitude())
                }).filter(|(_,_,_,distance)| *distance <= 22.).min_by(|a,b| a.3.total_cmp(&b.3));
                if let Some((first,p,h,_)) = candidate {
                    self.link_ui.click = None;
                    let (start,end) = self.core.ordered();
                    let caret = point(p.x,p.y+h/2.);
                    self.dismiss_touch_menu();
                    self.core.break_coalescing();
                    self.touch.drag = Some(Drag::Handle { before: self.core.sel,fixed: if first { end } else { start },offset: event.start_position-caret });
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }
            TouchPhase::Moved => {
                if let Some(Drag::Handle { fixed,offset,.. }) = self.touch.drag {
                    if let Some(pos) = self.pos_at(event.position-offset) { self.touch_extend(fixed,pos,cx); }
                    cx.stop_propagation();
                }
            }
            TouchPhase::Ended => {
                if matches!(self.touch.drag,Some(Drag::Handle { .. })) {
                    self.touch.drag = None;
                    self.present_touch_menu(event.position,cx);
                    cx.notify();
                    cx.stop_propagation();
                }
            }
            TouchPhase::Cancelled => self.cancel_touch_drag(cx),
            _ => {}
        }
    }
}

pub(super) fn paint(entity: &Entity<Editor>, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    if window.last_input_was_touch() && entity.read(cx).focus.is_focused(window) {
        let viewport = entity.read(cx).list.viewport_bounds();
        for (first,p,h) in entity.read(cx).touch_handles() {
            let knob = point(p.x,p.y+if first { -px(5.) } else { h+px(5.) });
            if viewport.contains(&knob) {
                window.paint_quad(fill(Bounds::new(point(p.x-px(1.),p.y),size(px(2.),h)),Theme::caret()));
                window.paint_quad(fill(Bounds::new(knob-point(px(5.),px(5.)),size(px(10.),px(10.))),Theme::caret()).corner_radii(px(5.)));
            }
        }
    }
    let e = entity.clone();
    window.on_mouse_event(move |event: &LongPressEvent,phase,window,cx| {
        if phase.bubble() { e.update(cx,|ed,cx| ed.touch_long_press(event,bounds,window,cx)); }
    });
    let e = entity.clone();
    window.on_mouse_event(move |event: &TouchDragEvent,phase,window,cx| {
        if phase.bubble() { e.update(cx,|ed,cx| ed.touch_handle_drag(event,window,cx)); }
    });
}

unsafe extern "C" {
    fn fei_ios_present_text_menu(json: *const std::ffi::c_char);
    fn fei_ios_dismiss_text_menu();
}
