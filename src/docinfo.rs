//! 文档信息 dialog: cut down to what a local file has (概览 times and
//! 文档统计). 访问记录 / 操作记录 / 隐私设置 and the
//! owner / interaction stats are cloud-only.

use crate::app::Workspace;
use crate::model::Document;
use crate::ui::{format_date, format_relative};
use gpui::{prelude::FluentBuilder, *};

#[derive(Clone, Debug, PartialEq)]
pub struct DocInfo {
    pub created: u64,
    pub modified: u64,
    pub words: usize,
    pub chars: usize,
    pub size: u64,
    /// Where the file is, and whether it was opened on its own (本机文件) rather than inside a folder.
    pub path: Option<std::path::PathBuf>,
    pub local: bool,
    /// The top-level folder it lives in (none for a file opened on its own).
    pub place: String,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x2fa1f
        | 0x3040..=0x30ff | 0xac00..=0xd7af)
}

/// 字数: each CJK character and full-width punctuation mark counts as one,
/// a run of Latin letters / digits counts as one word; ASCII punctuation and
/// spaces don't count.
pub fn word_count(s: &str) -> usize {
    let mut n = 0;
    let mut in_word = false;
    for c in s.chars() {
        let wordy = c.is_alphanumeric() && !is_cjk(c) || (in_word && matches!(c, '\'' | '’' | '-' | '.' | '_'));
        if wordy {
            if !in_word {
                n += 1;
            }
            in_word = true;
            continue;
        }
        in_word = false;
        if c.is_whitespace() || c.is_ascii() || c == '\u{fffc}' {
            continue;
        }
        n += 1;
    }
    n
}

/// `word_count` over blocks (children and table cells included) without
/// joining their text; block boundaries separate words anyway.
pub fn blocks_word_count(blocks: &[crate::model::Block]) -> usize {
    blocks
        .iter()
        .map(|b| {
            let cells = match &b.kind {
                crate::model::BlockKind::Table(t) => t.rows.iter().flatten().map(|c| blocks_word_count(&c.blocks)).sum(),
                _ => 0,
            };
            word_count(&b.text.text) + blocks_word_count(&b.children) + cells
        })
        .sum()
}

/// 字符数: every visible character (letters, digits, CJK, punctuation); no spaces.
pub fn char_count(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace() && *c != '\u{fffc}').count()
}

/// Title plus body, the text the counts run over.
pub fn doc_text(doc: &Document) -> String {
    format!("{}\n{}", doc.title.text, doc.plain_text())
}

pub fn format_size(n: u64) -> String {
    const K: f64 = 1024.;
    let f = n as f64;
    if f < K {
        format!("{n} B")
    } else if f < K * K {
        format!("{:.2} KB", f / K)
    } else if f < K * K * K {
        format!("{:.2} MB", f / K / K)
    } else {
        format!("{:.2} GB", f / K / K / K)
    }
}

impl Workspace {
    pub fn open_doc_info(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else { return };
        let Some(meta) = self.entries.iter().find(|e| e.id == id).map(|e| e.meta.clone()) else { return };
        let doc = &ed.read(cx).core.doc;
        let text = doc_text(doc);
        self.doc_info = Some(DocInfo {
            created: meta.created,
            modified: meta.modified,
            words: word_count(&text),
            chars: char_count(&text),
            size: self.store.size_of(&id, doc),
            path: self.store.file_path(&id).or_else(|| self.store.local_path(&id)),
            local: meta.local(),
            place: self.entries.iter().find(|e| e.id == id).map(|e| self.folder_name(&e.top)).unwrap_or_default(),
        });
        self.doc_menu = false;
        self.doc_sub = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// A `Dialog` laid out edge to edge: the title with a close button, the
    /// one selected nav item and soft tiles.
    pub(crate) fn render_doc_info(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let info = self.doc_info.clone()?;
        let section = |title: &'static str| {
            div().text_size(px(13.)).line_height(px(20.)).font_weight(FontWeight::MEDIUM).text_color(crate::design::color::foreground()).child(title)
        };
        let tile = |label: &'static str, value: String, big: bool| {
            div()
                .w(px(278.))
                .h(px(if big { 44. } else { 40. }))
                .pl(px(16.))
                .flex()
                .items_center()
                .gap(px(16.))
                .rounded(px(crate::design::R))
                .bg(crate::design::color::muted())
                .child(div().text_size(px(13.)).line_height(px(20.)).text_color(crate::design::color::muted_foreground()).child(label))
                .child(
                    div()
                        .text_size(px(if big { 19. } else { 13. }))
                        .line_height(px(if big { 26. } else { 20. }))
                        .when(big, |d| d.font_weight(FontWeight::SEMIBOLD))
                        .text_color(crate::design::color::foreground())
                        .child(value),
                )
        };
        let grid = |tiles: Vec<Div>| div().mt(px(12.)).flex().flex_wrap().gap_x(px(12.)).gap_y(px(8.)).children(tiles);
        let close = cx.listener(|w, _, _, cx| {
            w.doc_info = None;
            cx.notify();
        });
        let ws = cx.entity().downgrade();
        let modal = crate::design::Dialog::new("doc-info")
            .keys(&self.dialog_focus, move |_, cx| { let _ = ws.update(cx, |w, cx| { w.doc_info = None; cx.notify(); }); }, None::<fn(&mut Window, &mut App)>)
            .w(px(840.))
            .h(px(562.))
            .p_0()
            .gap_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .closable(close)
            .child(
                div()
                    .flex_none()
                    .h(px(72.))
                    .px(px(24.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(crate::design::color::border())
                    .child(div().text_size(px(16.)).line_height(px(20.)).child(crate::settings::tr("文档信息", "Document information"))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div().flex_none().w(px(224.)).h_full().pt(px(12.)).px(px(12.)).border_r_1().border_color(crate::design::color::border()).child(
                            // The one nav item, selected.
                            crate::design::Button::ghost("doc-info-nav").sized(crate::design::ButtonSize::Sm).w_full().justify_start().icon("list").label(crate::settings::text("信息与数据")).selected(true),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .p(px(24.))
                            .flex()
                            .flex_col()
                            .child(section(crate::settings::text("概览")))
                            .child(grid(vec![
                                tile(crate::settings::text("创建时间"), format_date(info.created), false),
                                tile(crate::settings::text("最近修改"), format_relative(info.modified), false),
                            ]))
                            .child(div().mt(px(24.)).child(section(crate::settings::text("文档统计"))))
                            .child(grid(vec![
                                tile(crate::settings::text("总字数"), info.words.to_string(), true),
                                tile(crate::settings::text("总字符数"), info.chars.to_string(), true),
                                tile(crate::settings::text("大小"), format_size(info.size), true),
                            ]))
                            // 存储位置: the file itself, edited where it is — what 打开 means, and what 导入 copied into.
                            .child(div().mt(px(24.)).child(section(crate::settings::tr("存储位置", "Location"))))
                            .when_some(info.path.clone(), |d, path| d.child(div().mt(px(12.)).w(px(568.)).px(px(16.)).py(px(10.)).rounded(px(crate::design::R)).bg(crate::design::color::muted())
                                .flex().items_center().gap(px(12.))
                                .child(div().flex_1().min_w_0().flex().flex_col()
                                    .child(div().text_size(px(13.)).line_height(px(20.)).text_color(crate::design::color::foreground()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(crate::ui::tilde(&path)))
                                    .child(div().text_size(px(12.)).line_height(px(18.)).text_color(crate::design::color::muted_foreground())
                                        .child(if info.local { crate::settings::tr("单独打开的文件：在原位置编辑，改动直接写回这个文件", "Opened on its own: edited where it is, changes go straight back to this file").to_string() }
                                            else { crate::localized_format!("在「{}」文件夹里：改动直接写入这个文件", "In the “{}” folder: changes go straight into this file", info.place) })))
                                .child(crate::design::Button::outline("doc-info-reveal").sized(crate::design::ButtonSize::Sm).label(crate::local_folders::show_in_files())
                                    .on_click(move |_, _, cx| cx.reveal_path(&path))))),
                    ),
            );
        Some(
            deferred(
                crate::design::scrim()
                    .id("doc-info-mask")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|w, _, _, cx| {
                            w.doc_info = None;
                            cx.notify();
                        }),
                    )
                    .child(modal),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{char_count, format_size, word_count};

    #[test]
    fn counts_like_feishu() {
        assert_eq!(word_count("你好，世界！"), 6);
        assert_eq!(word_count("hello world 2025"), 3);
        assert_eq!(word_count("飞书 docs, v1.5 版本."), 6);
        assert_eq!(char_count("飞书 docs, v1.5"), 11);
        assert_eq!(char_count("  \n"), 0);
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_898_000), "1.81 MB");
    }
}
