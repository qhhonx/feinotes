//! The main menu and the dropdown pieces, after Excalidraw
//! `main-menu/MainMenu.tsx`, `main-menu/DefaultItems.tsx` and
//! `dropdownMenu/*` @ 3a317b5: the hamburger (`.main-menu-trigger`) opens a
//! menu in the app's look (`design::Popover`, `MenuItem`: icon, text,
//! shortcut). The default items we support: 帮助 and 重置画布
//! (with its confirm dialog) and the canvas background (`ChangeCanvasBackground`).
//! Differs: no open / save / export / search / socials / theme items (the
//! app's header, above the board, has the way back and the document menu).

use super::*;
use crate::board::action::BoardAction;
use crate::settings::tr;
use crate::design::{self, MenuItem, MenuSeparator, Popover};

/// `DropdownMenuContent`: the app's menu (`design::Popover`).
pub(crate) fn dropdown(id: impl Into<ElementId>) -> Popover {
    eat(Popover::new(id).menu().max_w(px(320.)))
}

/// `DropdownMenuItem`: a menu row with the upstream icon and the shortcut at
/// the end; the current choice selected.
pub(crate) fn dropdown_item(id: impl Into<ElementId>, icon_name: Option<&str>, label: impl Into<SharedString>, shortcut: Option<String>, selected: bool) -> MenuItem {
    MenuItem::new(id).selected(selected)
        .when_some(icon_name, |d, n| d.leading(icon(n, 16., if selected { design::color::foreground() } else { design::color::muted_foreground() })))
        .label(label)
        .when_some(shortcut, |d, k| d.shortcut(k))
}

/// `DropdownMenuSeparator`.
pub(crate) fn dropdown_separator() -> MenuSeparator { MenuSeparator::new() }

/// Top left (`.excalidraw-ui-top-left`): the hamburger and its menu.
pub(crate) fn main_menu(me: &BoardView, window: &mut Window, cx: &mut Context<BoardView>) -> impl IntoElement {
    let open = matches!(pop(cx), Some(Pop::MainMenu | Pop::CanvasBackground));
    let trigger = filled_button("board-main-menu", open).w(px(style::LG_BUTTON))
        .child(icon("hamburger", style::ICON, style::text()))
        .on_click(cx.listener(|_, _, _, cx| { let open = matches!(pop(cx), Some(Pop::MainMenu | Pop::CanvasBackground)); set_pop(if open { None } else { Some(Pop::MainMenu) }, cx) }));
    // the board sits in the note pane, under the app's header (which names it)
    let mut col = div().absolute().top(px(style::TOP_PADDING)).left(px(style::PADDING)).child(trigger);
    if open {
        let run = |a: BoardAction| cx.listener(move |me: &mut BoardView, _: &ClickEvent, window, cx| { set_pop(None, cx); me.run(a.clone(), window, cx) });
        let menu = dropdown("board-main-menu-list").min_w(px(232.)).max_w(px(320.))
            .on_mouse_down_out(cx.listener(|_, _, _, cx| if pop(cx) == Some(Pop::MainMenu) { set_pop(None, cx) }))
            .child(dropdown_item("board-mm-help", Some("help"), tr("帮助", "Help"), Some("?".into()), false).on_click(run(BoardAction::ShowHelp)))
            // (upstream shows it on an empty canvas too)
            .child(dropdown_item("board-mm-clear", Some("trash"), tr("重置画布", "Reset the canvas"), None, false)
                .on_click(cx.listener(|_, _, _, cx| set_pop(Some(Pop::ClearCanvas), cx))))
            // `ChangeCanvasBackground`: 12 below the separator, a 14px label
            // (4 below, 8 in) over the picker 10 in
            .child(dropdown_separator())
            .child(div().mt(px(12.)).flex().flex_col()
                .child(div().mb(px(4.)).ml(px(8.)).text_size(px(14.)).line_height(style::lh(14.)).text_color(style::on_surface()).child(tr("画布背景", "Canvas background")))
                .child(div().px(px(10.)).child(super::color_picker::color_picker("board-canvas-bg", None, Pop::CanvasBackground, Some(me.app_state().view_background_color.clone()),
                    super::colors::CANVAS_BACKGROUND_PICKS, BoardAction::ViewBackgroundColor, me.elements(), window, cx))));
        // below the trigger, left edges aligned (`sideOffset` 8)
        col = col.child(div().absolute().top(px(style::LG_BUTTON + 8.)).left_0().child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(menu)).with_priority(3)));
    }
    col
}
