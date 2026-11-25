// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use gpui::prelude::*;
use gpui::*;
use crate::{design::{self, Button, ButtonSize, Dialog}, app::{View, Workspace}, editor::view::Editor, settings::tr, store::PageWidth, theme::Theme, ui};
use super::ConflictVersions;

pub(crate) struct ConflictReview {
    id: String,
    versions: ConflictVersions,
    received: bool,
    preview: Entity<Editor>,
}

fn touch_target(button: Button) -> Button {
    button.when(cfg!(target_os = "ios"), |b| b.h(px(44.)).min_w(px(44.)))
}

impl Workspace {
    pub(crate) fn has_conflict_banner(&self, cx: &App) -> bool {
        self.view == View::Doc && self.preview.is_none()
            && self.current.as_deref().is_some_and(|id| super::document_conflict(self.store.root(), id, cx))
    }

    pub(crate) fn render_conflict_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.has_conflict_banner(cx) { return None; }
        Some(div().id("document-sync-conflict").flex_none().w_full().px(px(16.)).py(px(5.)).flex().items_center()
            .justify_between().gap(px(8.)).bg(Theme::bg_body()).border_b_1().border_color(Theme::line_divider())
            .child(div().min_w_0().text_size(px(13.)).text_color(Theme::danger())
                .child(ui::ui_para(tr("此文档需要处理", "This document needs attention"))))
            .child(touch_target(Button::outline("review-sync-conflict").sized(ButtonSize::Sm).label(tr("查看版本", "Review versions")))
                .on_click(cx.listener(|w, _, window, cx| w.review_conflict(window, cx))))
            .into_any_element())
    }

    fn review_conflict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        {
            use gpui_mobile::ios::ffi;
            ffi::gpui_ios_hide_keyboard(ffi::gpui_ios_get_window());
        }
        if !self.flush(cx) { return; }
        let Some(id) = self.current.clone() else { return };
        let root = self.store.root().to_path_buf();
        match super::conflict_versions(&root, &id, cx) {
            Ok(versions) => {
                let preview = cx.new(|cx| {
                    let mut editor = Editor::new(versions.local.document.clone(), 0, PageWidth::Full, window, cx);
                    editor.set_library_root(root);
                    editor.read_only = true;
                    editor.preview = true;
                    editor
                });
                self.conflict_review = Some(ConflictReview { id, versions, received: false, preview });
                cx.notify();
            }
            Err(_) => self.show_toast(tr("暂时无法查看冲突版本，请稍后重试。", "The conflicting versions are unavailable. Try again later."), cx),
        }
    }

    fn show_conflict_version(&mut self, received: bool, cx: &mut Context<Self>) {
        let Some(review) = &mut self.conflict_review else { return };
        if received == review.received { return; }
        let version = if received { review.versions.received.as_ref() } else { Some(&review.versions.local) };
        let Some(version) = version else { return };
        review.received = received;
        review.preview.update(cx, |editor, cx| {
            editor.receive_document(&crate::editor::core::Core::new(version.document.clone()), cx);
        });
        cx.notify();
    }

    fn save_conflict_copies(&mut self, both: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = &self.conflict_review else { return };
        let id = review.id.clone();
        let choices = [(false, Some(&review.versions.local)), (true, review.versions.received.as_ref())];
        let chosen: Vec<_> = choices.into_iter().filter(|(received, _)| both || *received == review.received)
            .filter_map(|(received, version)| version.map(|v| (received, v.stamp))).collect();
        if chosen.len() != if both { 2 } else { 1 } { return; }
        let root = self.store.root().to_path_buf();
        match super::copy_conflict_versions(&root, &id, &chosen, cx) {
            Ok(copies) if !copies.is_empty() => {
                self.conflict_review = None;
                self.entries = self.store.list();
                self.sort();
                self.open(&copies[0], window, cx);
                self.show_toast(tr("已保存恢复副本，原文档仍保留。", "Recovery copies saved. The original is still available."), cx);
            }
            _ => {
                self.review_conflict(window, cx);
                self.show_toast(tr("无法保存副本。请检查最新版本和附件。", "Could not save a copy. Check the latest versions and attachments."), cx);
            }
        }
    }

    pub(crate) fn render_conflict_review(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let review = self.conflict_review.as_ref()?;
        if self.view != View::Doc || self.current.as_ref() != Some(&review.id)
            || !super::can_review_conflict(self.store.root(), cx)
            || !super::document_conflict(self.store.root(), &review.id, cx) { return None; }
        let remote = review.versions.received.as_ref();
        let copyable = if review.received { remote.is_some_and(|v| v.copyable) } else { review.versions.local.copyable };
        let both = review.versions.local.copyable && remote.is_some_and(|v| v.copyable);
        let choice = |received: bool, id, label, cx: &mut Context<Self>| {
            touch_target(Button::outline(id).sized(ButtonSize::Sm).label(label))
                .selected(review.received == received).accent(Theme::primary(), Theme::text())
                .on_click(cx.listener(move |w, _, _, cx| w.show_conflict_version(received, cx)))
        };
        let heading = div().p(px(14.)).flex_none()
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD)
                .child(tr("此文档需要处理", "This document needs attention")))
            .child(div().mt(px(4.)).text_size(px(13.)).text_color(Theme::text_caption()).child(ui::ui_para(tr(
                "飞记已保留此文档的改动。你可以查看版本，其他文档会继续同步。",
                "Fei Notes kept the changes to this document. You can review the versions while other documents continue to sync."))))
            .child(div().mt(px(8.)).flex().flex_wrap().gap(px(6.))
                .child(choice(false, "conflict-local", tr("本机修改", "On this device"), cx))
                .when(remote.is_some(), |d| d.child(choice(true, "conflict-cloud", tr("收到的 iCloud 版本", "Received from iCloud"), cx))))
            .when(remote.is_none(), |d| d.child(div().mt(px(4.)).text_size(px(12.)).text_color(Theme::text_caption())
                .child(ui::ui_para(tr("此时无法预览收到的版本。", "The received version is unavailable for preview.")))));
        let heading = heading.when(!self.store.is_shared() && !copyable, |d| d.child(
            div().mt(px(4.)).text_size(px(12.)).text_color(Theme::text_caption()).child(ui::ui_para(tr(
                "此版本的附件尚未齐全，暂时不能另存。",
                "A copy requires all attachments for this version.")))));
        let viewport = window.viewport_size();
        let dialog = Dialog::new("document-sync-review").p_0().gap_0().w((viewport.width - px(24.)).min(px(680.)))
            .h((viewport.height - px(24.)).min(px(620.))).flex().flex_col().overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(heading)
            .child(div().flex_1().min_h_0().overflow_hidden().border_y_1().border_color(Theme::line_divider()).child(review.preview.clone()))
            .child(div().flex_none().p(px(12.)).flex().flex_wrap().justify_end().gap(px(6.))
                .child(touch_target(Button::outline("conflict-later").sized(ButtonSize::Sm).label(tr("稍后处理", "Later")))
                    .on_click(cx.listener(|w, _, _, cx| { w.conflict_review = None; cx.notify(); })))
                .child(touch_target(Button::outline("retry-document-sync").sized(ButtonSize::Sm).label(tr("重试同步", "Retry sync")))
                    .on_click(cx.listener(|w, _, _, cx| {
                        let Some(review) = &w.conflict_review else { return };
                        if super::retry_document(w.store.root(), &review.id, cx).is_ok() {
                            w.conflict_review = None;
                            cx.notify();
                        }
                    })))
                .when(copyable, |d| d.child(touch_target(Button::outline("copy-conflict-version").sized(ButtonSize::Sm).label(tr("另存此版本", "Save this version")))
                    .on_click(cx.listener(|w, _, window, cx| w.save_conflict_copies(false, window, cx)))))
                .when(both, |d| d.child(touch_target(Button::primary("keep-conflict-versions").sized(ButtonSize::Sm)
                    .accent(Theme::primary(), super::presence::badge_ink(Theme::primary()))
                    .label(tr("将两份另存为副本", "Save both as copies")))
                    .on_click(cx.listener(|w, _, window, cx| w.save_conflict_copies(true, window, cx))))));
        Some(deferred(design::scrim().id("sync-conflict-mask").occlude().child(dialog)).with_priority(22).into_any_element())
    }
}
