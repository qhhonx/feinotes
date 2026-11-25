//! 导入思源笔记: the app's side of `fdl_core::siyuan` — the text it writes
//! in the interface's language, one run shared by the 导入 menu and 欢迎页,
//! the toast that ends it, and the 导入报告 page listing what changed form.

use crate::model::{Block, BlockKind, Document, InlineDocLink, InlineKind, Marks, RichText};
use crate::settings::tr;
use fdl_core::siyuan::{self, Report};
use std::path::Path;

fn labels() -> siyuan::Labels {
    siyuan::Labels {
        attributes: tr("思源笔记属性", "SiYuan attributes").into(),
        block: tr("块", "Block").into(),
        name: tr("属性", "Attribute").into(),
        value: tr("值", "Value").into(),
        document: tr("文档", "Document").into(),
        query: tr("思源笔记嵌入块（查询结果没有导入）：", "SiYuan embed (its query results were not imported):").into(),
    }
}

/// Imports the workspace, notebook or `.sy.zip` at `path` into `store`
/// under `parent`: one notebook as a folder, several inside one 思源笔记
/// folder, with a 导入报告 page there when something changed form. What to
/// show (that page, else the folder), and what happened. Call it inside a batch.
pub(crate) fn import(store: &mut crate::store::Store, path: &Path, parent: Option<&str>) -> anyhow::Result<(Option<String>, Report)> {
    let src = siyuan::open(path)?;
    let parent = if src.notebooks.len() > 1 {
        let id = store.create_folder(tr("思源笔记", "SiYuan"), parent)?;
        if parent.is_some() { let _ = store.set_parent(&id, parent); }
        Some(id)
    } else { parent.map(str::to_string) };
    let wrapped = src.notebooks.len() > 1;
    let report = siyuan::import(store, &src, parent.as_deref(), &labels(), &mut siyuan::StaticTables)?;
    let top = if wrapped { parent } else { report.folders.first().cloned() };
    let page = if noteworthy(&report) && let Some(top) = &top { store.create_from_in(&report_page(&report), Some(top)).ok() } else { None };
    Ok((page.or(top), report))
}

/// Whether the import changed anything a reader would look for.
fn noteworthy(r: &Report) -> bool {
    !r.degraded.is_empty() || !r.block_refs.is_empty() || !r.broken_refs.is_empty() || !r.databases.is_empty() || r.queries > 0 || !r.missing_assets.is_empty() || !r.failed.is_empty()
}

/// The toast after an import: how many pages, and how many could not be
/// read (an error then). It stays one line; the report page, shown after
/// it, lists what changed form.
pub(crate) fn summary(r: &Report) -> (bool, String) {
    let mut text = crate::localized_count!(r.docs, "已复制 {} 篇思源笔记文档，原文件未改动", "Copied {} SiYuan document; the originals are untouched", "Copied {} SiYuan documents; the originals are untouched", r.docs);
    if !r.failed.is_empty() { text.push_str(&crate::localized_count!(r.failed.len(), "，{} 篇未能导入", ", {} failed", ", {} failed", r.failed.len())); }
    (!r.failed.is_empty(), text)
}

/// What a degraded 思源 type became, for the report.
fn degraded_name(key: &str) -> String {
    let fixed = match key {
        "kbd" => Some(tr("键盘按键（kbd）→ 行内代码", "Keyboard keys (kbd) → inline code")),
        "sup / sub" => Some(tr("上标、下标 → 普通文字", "Superscript, subscript → plain text")),
        "inline-memo" => Some(tr("行内备注 → 文字后的灰色括注", "Inline memos → gray note after the text")),
        "file-annotation-ref" => Some(tr("PDF 标注引用 → 普通文字", "PDF annotation references → plain text")),
        "NodeTabs" => Some(tr("标签页 → 加粗的页名和缩进的内容", "Tabs → bold tab name with its content indented")),
        "NodeMindmap" => Some(tr("思维导图 → 无序列表", "Mind maps → bulleted list")),
        "NodeHTMLBlock" => Some(tr("HTML 块 → HTML 代码块", "HTML blocks → HTML code blocks")),
        "NodeIFrame" => Some(tr("iframe → 链接", "iframes → links")),
        "NodeWidget" => Some(tr("挂件 → 链接", "Widgets → links")),
        "NodeVideo" | "NodeAudio" => Some(tr("没有找到文件的视频、音频 → 链接", "Video and audio without their file → links")),
        "NodeSuperBlock col" => Some(tr("分栏里的分栏 → 依次排列", "Columns inside columns → one after another")),
        "NodeAttributeView" => Some(tr("没有找到数据的数据库 → 数据库 ID", "Databases without their data → database ID")),
        "NodeFootnotesDef" | "NodeFootnotesRef" => Some(tr("脚注 → [^n] 文字", "Footnotes → [^n] text")),
        "NodeInlineHTML" => Some(tr("行内 HTML → 文字", "Inline HTML → text")),
        "NodeEmojiImg" => Some(tr("自定义表情 → 文字", "Custom emoji → text")),
        _ => None,
    };
    if let Some(s) = fixed { return s.to_string() }
    match key.strip_prefix("NodeCodeBlock ") {
        Some(lang) => crate::localized_format!("{} 图表 → 代码块", "{} charts → code blocks", lang),
        None => crate::localized_format!("{} → 文字或其中的块", "{} → its text or blocks", key),
    }
}

/// 思源笔记导入报告: what became something else, each reference that is
/// not a link to what it pointed at, the databases now tables.
fn report_page(r: &Report) -> Document {
    let h2 = |s: &str| Block::new(BlockKind::Heading { level: 2, seq_auto: false }, RichText::plain(s));
    let bullet = |t: RichText| Block::new(BlockKind::Bullet, t);
    let mut blocks = vec![Block::text(&crate::localized_count!(r.docs, "从思源笔记导入了 {} 篇文档。", "Imported {} document from SiYuan.", "Imported {} documents from SiYuan.", r.docs))];
    if !r.degraded.is_empty() {
        blocks.push(h2(tr("换成相近格式的内容", "Content in the nearest form")));
        blocks.push(Block::text(tr("这些内容 Quiet.md 没有对应的格式，换成了最接近的写法，文字都还在。", "Quiet.md has no form for these, so they use the nearest one; their text is all there.")));
        for (k, n) in &r.degraded { blocks.push(bullet(RichText::plain(crate::localized_format!("{}：{} 处", "{}: {}", degraded_name(k), n)))); }
    }
    let refs = |list: &[siyuan::Ref], out: &mut Vec<Block>| for x in list {
        let mut t = RichText::default();
        t.push(crate::editor::core::EQUATION, Marks { inline: Some(crate::docxml::to_inline(InlineKind::DocLink(InlineDocLink { id: x.page.clone(), title: x.doc.clone() }))), ..Default::default() });
        t.push(&format!("{}{}", tr("：", ": "), x.text), Marks::default());
        out.push(bullet(t));
    };
    if !r.block_refs.is_empty() {
        blocks.push(h2(tr("改为链接到所在文档的块引用", "Block references now linking to their document")));
        blocks.push(Block::text(tr("这些引用指向文档里的某一块。Quiet.md 只能链接到文档或标题，所以改为链接到块所在的文档，引用的文字不变；指向同一文档的保留为文字。", "These referenced one block in a document. Quiet.md links to documents and headings only, so they link to the block’s document and keep their text; those within the same document stay text.")));
        refs(&r.block_refs, &mut blocks);
    }
    if !r.broken_refs.is_empty() {
        blocks.push(h2(tr("找不到目标的引用", "References without a target")));
        blocks.push(Block::text(tr("引用的块不在这次导入的内容里，保留了引用的文字。", "The block they referenced was not in what was imported; their text stays.")));
        refs(&r.broken_refs, &mut blocks);
    }
    if !r.databases.is_empty() {
        blocks.push(h2(tr("转成静态表格的数据库", "Databases turned into static tables")));
        blocks.push(Block::text(tr("数据库在原来的位置成了普通表格，内容是导入时视图里显示的样子，不会再随数据更新。", "Each database is now an ordinary table where it was, showing what its view showed at import; it no longer updates.")));
        for d in &r.databases { blocks.push(bullet(RichText::plain(d))); }
    }
    if r.embeds + r.queries > 0 {
        blocks.push(h2(tr("嵌入块", "Embeds")));
        if r.embeds > 0 { blocks.push(bullet(RichText::plain(crate::localized_count!(r.embeds, "{} 个按块 ID 嵌入的块复制了一份内容", "{} embed by block ID is now a copy of its content", "{} embeds by block ID are now copies of their content", r.embeds)))); }
        if r.queries > 0 { blocks.push(bullet(RichText::plain(crate::localized_count!(r.queries, "{} 个查询嵌入块保留了查询语句，没有导入查询结果", "{} query embed keeps its query; its results were not imported", "{} query embeds keep their queries; their results were not imported", r.queries)))); }
    }
    if !r.missing_assets.is_empty() {
        blocks.push(h2(tr("没有找到的图片和附件", "Images and files not found")));
        for a in &r.missing_assets { blocks.push(bullet(RichText::plain(a))); }
    }
    if !r.failed.is_empty() {
        blocks.push(h2(tr("未能导入的文档", "Documents not imported")));
        for f in &r.failed { blocks.push(bullet(RichText::plain(f))); }
    }
    if r.attributes > 0 {
        blocks.push(h2(tr("属性", "Attributes")));
        blocks.push(Block::text(&crate::localized_count!(r.attributes, "{} 个属性（命名、别名、备注、书签、标签和自定义属性）列在各自文档末尾折叠的「思源笔记属性」标题下。", "{} attribute (name, alias, memo, bookmark, tags and custom attributes) is listed under the folded “SiYuan attributes” heading at the end of its document.", "{} attributes (names, aliases, memos, bookmarks, tags and custom attributes) are listed under the folded “SiYuan attributes” heading at the end of their documents.", r.attributes)));
    }
    Document { title: RichText::plain(tr("思源笔记导入报告", "SiYuan Import Report")), icon: None, cover: None, blocks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_page_lists_what_changed_form() {
        let mut r = Report { docs: 3, embeds: 1, queries: 2, attributes: 4, ..Default::default() };
        r.degraded.insert("kbd".into(), 5);
        r.degraded.insert("NodeCodeBlock echarts".into(), 1);
        r.block_refs.push(siyuan::Ref { doc: "甲".into(), page: "id-1".into(), text: "某段".into(), target: "20240101000000-aaaaaaa".into() });
        r.databases.push("书单".into());
        let xml = crate::docxml::serialize(&report_page(&r));
        // In whichever language the run shows.
        for s in [degraded_name("kbd"), degraded_name("NodeCodeBlock echarts"), "<cite type=\"doc\" doc-id=\"id-1\" file-type=\"docx\">甲</cite>".into(), "某段".into(), "<li>书单</li>".into()] {
            assert!(xml.contains(&s), "{s}\n{xml}");
        }
        // 换成相近格式的内容, 块引用, 数据库, 嵌入块, 属性.
        assert_eq!(xml.matches("<h2>").count(), 5, "{xml}");
        let (error, text) = summary(&r);
        assert!(!error && text.contains('3'), "{text}");
        let (error, text) = summary(&Report { docs: 2, failed: vec!["x".into()], ..Default::default() });
        assert!(error && text.contains('2') && text.contains('1'), "{text}");
        assert!(!noteworthy(&Report { docs: 1, attributes: 2, embeds: 1, ..Default::default() }));
    }
}
