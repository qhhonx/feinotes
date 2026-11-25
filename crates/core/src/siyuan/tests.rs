//! One test per 思源 block type and inline mark: a node as Lute writes it
//! (shapes from lute's ast/node.go and 思源's 用户指南) → the DocxXML it becomes.

use super::*;
use serde_json::json;

/// Pages made from 思源 document trees, in `/lib/<title>.xml`, with what the import noted.
fn convert(trees: Vec<Value>, assets: Option<&Path>) -> (Vec<Document>, Report) {
    let docs: Vec<Doc> = trees.into_iter().map(|tree| {
        let sy = st(&tree, "ID").to_string();
        let title = prop(&tree, "title").to_string();
        Doc { sy, file: PathBuf::from("/nb/x.sy"), tree, title, parent: None, notebook: 0 }
    }).collect();
    let src = Source { notebooks: vec![], assets: assets.map(Path::to_path_buf).into_iter().collect(), avs: assets.map(|a| a.join("av")).into_iter().collect(), unpacked: None };
    let index = index(&docs);
    let lib: Vec<String> = (0..docs.len()).map(|i| format!("lib-{i}")).collect();
    let files: Vec<Option<PathBuf>> = docs.iter().map(|d| Some(PathBuf::from(format!("/lib/{}.xml", d.title)))).collect();
    let mut report = Report::default();
    let root = crate::vfs::temp_dir().join(format!("qmd-siyuan-test-{}", uuid::Uuid::new_v4()));
    let mut assets = Assets::new(root.clone());
    let labels = Labels::default();
    let out = (0..docs.len()).map(|at| Conv { src: &src, docs: &docs, index: &index, lib: &lib, files: &files, at, labels: &labels, dbs: &mut StaticTables, report: &mut report, assets: &mut assets, attrs: vec![], depth: 0, in_column: false }.document()).collect();
    let _ = crate::vfs::remove_dir_all(&root);
    (out, report)
}

fn page(id: &str, title: &str, blocks: Vec<Value>) -> Value {
    json!({"ID": id, "Type": "NodeDocument", "Properties": {"id": id, "title": title, "updated": "20240102030405"}, "Children": blocks})
}

/// The page's body as DocxXML (without its title line).
fn body(doc: &Document) -> String {
    let xml = crate::docxml::serialize(doc);
    xml.split_once('\n').map_or(String::new(), |(_, b)| b.to_string())
}

/// `blocks` on a page by themselves → its body, and the report.
fn one(blocks: Vec<Value>) -> (String, Report) {
    let (docs, report) = convert(vec![page("20240101000000-aaaaaaa", "页", blocks)], None);
    (body(&docs[0]), report)
}

fn text(s: &str) -> Value { json!({"Type": "NodeText", "Data": s}) }
fn para(kids: Vec<Value>) -> Value { json!({"ID": "20240101000001-ppppppp", "Type": "NodeParagraph", "Properties": {}, "Children": kids}) }
fn mark(types: &str, content: &str) -> Value { json!({"Type": "NodeTextMark", "TextMarkType": types, "TextMarkTextContent": content}) }

#[test]
fn paragraph_and_heading() {
    let (xml, _) = one(vec![
        json!({"ID": "20240101000002-hhhhhhh", "Type": "NodeHeading", "HeadingLevel": 2, "Properties": {"fold": "1"}, "Children": [text("二级")]}),
        para(vec![text("一段\u{200b}"), json!({"Type": "NodeBr", "Data": "br"}), text("第二行")]),
        json!({"Type": "NodeParagraph", "Properties": {"style": "text-align: center; background-color: var(--b3-font-background3);"}, "Children": [text("居中")]}),
    ]);
    assert_eq!(xml, "<h2 folded=\"true\">二级</h2>\n<p>一段<br/>第二行</p>\n<p align=\"center\" background-color=\"light-blue\">居中</p>\n");
}

#[test]
fn lists_ordered_bullet_task_and_folded_items() {
    let item = |t: &str, data: Value, extra: Vec<Value>, props: Value| {
        let mut k = extra;
        k.push(para(vec![text(t)]));
        json!({"Type": "NodeListItem", "ListData": data, "Properties": props, "Children": k})
    };
    let (xml, _) = one(vec![
        json!({"Type": "NodeList", "ListData": {"Typ": 1, "Start": 3}, "Children": [item("三", json!({"Typ": 1}), vec![], json!({})), item("四", json!({"Typ": 1}), vec![], json!({}))]}),
        json!({"Type": "NodeList", "ListData": {}, "Children": [
            {"Type": "NodeListItem", "ListData": {}, "Properties": {"fold": "1"}, "Children": [para(vec![text("父")]),
                {"Type": "NodeList", "ListData": {}, "Children": [item("子", json!({}), vec![], json!({}))]}]}]}),
        json!({"Type": "NodeList", "ListData": {"Typ": 3}, "Children": [
            item("做完", json!({"Typ": 3}), vec![json!({"Type": "NodeTaskListItemMarker", "Data": "[X]", "TaskListItemChecked": true})], json!({})),
            item("没做", json!({"Typ": 3}), vec![json!({"Type": "NodeTaskListItemMarker", "Data": "[ ]"})], json!({}))]}),
    ]);
    assert_eq!(xml, "<ol>\n  <li seq=\"3\">三</li>\n  <li>四</li>\n</ol>\n<ul>\n  <li folded=\"true\">父\n    <ul>\n      <li>子</li>\n    </ul>\n  </li>\n</ul>\n<checkbox done=\"true\">做完</checkbox>\n<checkbox done=\"false\">没做</checkbox>\n");
}

#[test]
fn quote_callout_and_divider() {
    let (xml, _) = one(vec![
        json!({"Type": "NodeBlockquote", "Children": [{"Type": "NodeBlockquoteMarker", "Data": "> "}, para(vec![text("引")])]}),
        json!({"Type": "NodeCallout", "CalloutType": "WARNING", "CalloutTitle": "注意", "CalloutIcon": "26a0-fe0f", "Children": [para(vec![text("小心")])]}),
        json!({"Type": "NodeThematicBreak"}),
    ]);
    assert_eq!(xml, "<blockquote>\n  <p>引</p>\n</blockquote>\n<callout emoji=\"⚠️\" background-color=\"light-yellow\" border-color=\"yellow\">\n  <p><b>注意</b></p>\n  <p>小心</p>\n</callout>\n<hr/>\n");
}

#[test]
fn code_math_and_html() {
    let (xml, report) = one(vec![
        json!({"Type": "NodeCodeBlock", "CodeBlockInfo": "cnVzdA==", "Children": [{"Type": "NodeCodeBlockCode", "Data": "fn main() {}\n"}]}),
        json!({"Type": "NodeCodeBlock", "CodeBlockInfo": "ZWNoYXJ0cw==", "Children": [{"Type": "NodeCodeBlockCode", "Data": "{}\n"}]}),
        json!({"Type": "NodeMathBlock", "Children": [{"Type": "NodeMathBlockOpenMarker"}, {"Type": "NodeMathBlockContent", "Data": "a^2+b^2"}, {"Type": "NodeMathBlockCloseMarker"}]}),
        json!({"Type": "NodeHTMLBlock", "Data": "<div>hi</div>"}),
    ]);
    assert_eq!(xml, "<pre lang=\"rust\"><code>fn main() {}</code></pre>\n<pre lang=\"echarts\"><code>{}</code></pre>\n<p align=\"center\"><latex>a^2+b^2</latex></p>\n<pre lang=\"html\"><code>&lt;div&gt;hi&lt;/div&gt;</code></pre>\n");
    assert_eq!(report.degraded.get("NodeCodeBlock echarts"), Some(&1));
    assert_eq!(report.degraded.get("NodeHTMLBlock"), Some(&1));
}

#[test]
fn inline_marks() {
    let mut color = mark("strong", "红");
    color["Properties"] = json!({"style": "color: var(--b3-font-color1); background-color: var(--b3-font-background12);"});
    let mut math = mark("inline-math", "");
    math["TextMarkInlineMathContent"] = json!("x^2");
    let mut link = mark("a code", "站");
    link["TextMarkAHref"] = json!("https://b3log.org");
    let mut memo = mark("inline-memo", "词");
    memo["TextMarkInlineMemoContent"] = json!("解释");
    let (xml, report) = one(vec![para(vec![
        mark("strong em", "粗斜"), mark("u", "下"), mark("s", "删"), mark("mark", "亮"), mark("code", "c"), mark("kbd", "⌘K"),
        mark("sup", "上"), mark("sub", "下标"), mark("tag", "读书"), color, math, link, memo,
    ])]);
    assert_eq!(xml, "<p><b><em>粗斜</em></b><u>下</u><del>删</del><span background-color=\"light-yellow\">亮</span><code>c⌘K</code>上下标#读书<span text-color=\"red\" background-color=\"light-yellow\"><b>红</b></span><latex>x^2</latex><a href=\"https://b3log.org\"><code>站</code></a>词<span text-color=\"gray\">（解释）</span></p>\n");
    assert_eq!((report.degraded["kbd"], report.degraded["sup / sub"], report.degraded["inline-memo"]), (1, 2, 1));
}

#[test]
fn superblocks_become_columns_or_follow_each_other() {
    let sb = |layout: &str, kids: Vec<Value>| {
        let mut k = vec![json!({"Type": "NodeSuperBlockOpenMarker"}), json!({"Type": "NodeSuperBlockLayoutMarker", "Data": layout})];
        k.extend(kids);
        k.push(json!({"Type": "NodeSuperBlockCloseMarker"}));
        json!({"ID": "20240101000003-sssssss", "Type": "NodeSuperBlock", "Children": k})
    };
    let (xml, report) = one(vec![
        sb("col", vec![sb("row", vec![para(vec![text("左一")]), para(vec![text("左二")])]), sb("col", vec![para(vec![text("右甲")]), para(vec![text("右乙")])])]),
        sb("row", vec![para(vec![text("上")]), para(vec![text("下")])]),
    ]);
    assert_eq!(xml, "<grid>\n  <column width-ratio=\"0.5\">\n    <p>左一</p>\n    <p>左二</p>\n  </column>\n  <column width-ratio=\"0.5\">\n    <p>右甲</p>\n    <p>右乙</p>\n  </column>\n</grid>\n<p>上</p>\n<p>下</p>\n");
    assert_eq!(report.degraded.get("NodeSuperBlock col"), Some(&1));
}

#[test]
fn tables_keep_header_alignment_widths_and_merges() {
    let cell = |t: &str, props: Value| json!({"Type": "NodeTableCell", "Data": "td", "Properties": props, "Children": [text(t)]});
    let (xml, _) = one(vec![json!({"Type": "NodeTable", "TableAligns": [0, 2], "Properties": {"colgroup": "width: 120px|"},
        "Children": [
            {"Type": "NodeTableHead", "Children": [{"Type": "NodeTableRow", "Children": [cell("甲", json!({})), cell("乙", json!({}))]}]},
            {"Type": "NodeTableRow", "Children": [cell("合并", json!({"colspan": "2", "style": "background-color: var(--b3-font-background4);"}))]},
            {"Type": "NodeTableRow", "Children": [cell("1", json!({})), cell("2", json!({}))]}]})]);
    assert_eq!(xml, "<table>\n  <colgroup><col width=\"120\"/><col width=\"50\"/></colgroup>\n  <thead>\n    <tr><th><p>甲</p></th><th><p align=\"center\">乙</p></th></tr>\n  </thead>\n  <tbody>\n    <tr><td background-color=\"light-green\" colspan=\"2\"><p>合并</p></td></tr>\n    <tr><td><p>1</p></td><td><p align=\"center\">2</p></td></tr>\n  </tbody>\n</table>\n");
}

#[test]
fn tabs_mindmaps_iframes_and_unknown_blocks_keep_their_content() {
    let (xml, report) = one(vec![
        json!({"Type": "NodeTabs", "Children": [{"Type": "NodeTabItem", "TabItemTitle": "第一页", "Children": [para(vec![text("甲")])]}]}),
        json!({"Type": "NodeMindmap", "ListData": {}, "Children": [{"Type": "NodeMindmapItem", "ListData": {}, "Children": [para(vec![text("中心")])]}]}),
        json!({"Type": "NodeIFrame", "Data": "<iframe src=\"//player.example.com/v?a=1&amp;b=2\"></iframe>"}),
        json!({"Type": "NodeWhatever", "Data": "新东西"}),
    ]);
    assert_eq!(xml, "<p><b>第一页</b>\n  <p>甲</p>\n</p>\n<ul>\n  <li>中心</li>\n</ul>\n<p><a href=\"https://player.example.com/v?a=1&amp;b=2\">https://player.example.com/v?a=1&amp;b=2</a></p>\n<p>新东西</p>\n");
    for k in ["NodeTabs", "NodeMindmap", "NodeIFrame", "NodeWhatever"] { assert_eq!(report.degraded.get(k), Some(&1), "{k}"); }
}

#[test]
fn references_to_documents_headings_and_blocks() {
    let r = |id: &str, t: &str, sub: &str| { let mut m = mark("block-ref", t); m["TextMarkBlockRefID"] = json!(id); m["TextMarkBlockRefSubtype"] = json!(sub); m };
    let target = page("20240101000010-ttttttt", "目标 页", vec![
        json!({"ID": "20240101000011-hhhhhhh", "Type": "NodeHeading", "HeadingLevel": 1, "Children": [text("一节 #1")]}),
        json!({"ID": "20240101000012-bbbbbbb", "Type": "NodeParagraph", "Children": [text("某段")]}),
    ]);
    let from = page("20240101000020-fffffff", "来源", vec![para(vec![
        r("20240101000010-ttttttt", "目标 页", "d"), r("20240101000010-ttttttt", "别名", "s"), r("20240101000011-hhhhhhh", "一节 #1", "d"),
        r("20240101000012-bbbbbbb", "某段", "s"), r("20240101000099-mmmmmmm", "丢了", "s"),
    ])]);
    let (docs, report) = convert(vec![target, from], None);
    assert_eq!(body(&docs[1]), "<p><cite type=\"doc\" doc-id=\"lib-0\" file-type=\"docx\">目标 页</cite><a href=\"目标%20页.xml\">别名</a><a href=\"目标%20页.xml#一节%20%231\">一节 #1</a><a href=\"目标%20页.xml\">某段</a>丢了</p>\n".to_string());
    assert_eq!(report.block_refs, [Ref { doc: "来源".into(), page: "lib-1".into(), text: "某段".into(), target: "20240101000012-bbbbbbb".into() }]);
    assert_eq!(report.broken_refs, [Ref { doc: "来源".into(), page: "lib-1".into(), text: "丢了".into(), target: "20240101000099-mmmmmmm".into() }]);
}

#[test]
fn embeds_copy_their_blocks_or_keep_their_query() {
    let target = page("20240101000010-ttttttt", "目标", vec![
        json!({"ID": "20240101000011-hhhhhhh", "Type": "NodeHeading", "HeadingLevel": 2, "Children": [text("节")]}),
        json!({"ID": "20240101000012-bbbbbbb", "Type": "NodeParagraph", "Children": [text("节里")]}),
        json!({"ID": "20240101000013-ccccccc", "Type": "NodeHeading", "HeadingLevel": 2, "Children": [text("下一节")]}),
    ]);
    let embed = |q: &str| json!({"Type": "NodeBlockQueryEmbed", "Data": format!("{{{{{q}}}}}"), "Children": [{"Type": "NodeOpenBrace"}, {"Type": "NodeBlockQueryEmbedScript", "Data": q}]});
    let from = page("20240101000020-fffffff", "来源", vec![
        embed("select * from blocks where id='20240101000011-hhhhhhh'"),
        embed("SELECT * FROM blocks WHERE content LIKE '%x%'"),
    ]);
    let (docs, report) = convert(vec![target, from], None);
    assert_eq!(body(&docs[1]), "<h2>节</h2>\n<p>节里</p>\n<p>思源笔记嵌入块（查询结果没有导入）：</p>\n<pre lang=\"sql\"><code>SELECT * FROM blocks WHERE content LIKE '%x%'</code></pre>\n");
    assert_eq!((report.embeds, report.queries), (1, 1));
}

#[test]
fn attributes_icon_and_times() {
    let mut doc = page("20240101000000-aaaaaaa", "页", vec![json!({"ID": "20240101000001-ppppppp", "Type": "NodeParagraph",
        "Properties": {"name": "要点", "custom-评分": "5", "custom-sy-hidden": "x"}, "Children": [text("一段")]})]);
    doc["Properties"]["icon"] = json!("1f4d6");
    doc["Properties"]["alias"] = json!("别名");
    doc["Properties"]["custom-project"] = json!("A");
    let (docs, report) = convert(vec![doc], None);
    assert_eq!(docs[0].icon.as_deref(), Some("📖"));
    let xml = body(&docs[0]);
    assert!(xml.starts_with("<p>一段</p>\n<h2 folded=\"true\">思源笔记属性</h2>\n<table>"), "{xml}");
    for row in ["<td><p>一段</p></td><td><p>name</p></td><td><p>要点</p></td>", "<td><p>一段</p></td><td><p>评分</p></td><td><p>5</p></td>", "<td><p>文档</p></td><td><p>alias</p></td><td><p>别名</p></td>", "<td><p>文档</p></td><td><p>project</p></td><td><p>A</p></td>"] {
        assert!(xml.contains(row), "{row}\n{xml}");
    }
    assert!(!xml.contains("sy-hidden"));
    assert_eq!(report.attributes, 4);
    // Only a page with attributes of its writer's gets the section: none for 思源's own.
    let mut daily = page("20240101000000-bbbbbbb", "2024-01-01", vec![json!({"ID": "20240101000001-qqqqqqq", "Type": "NodeParagraph",
        "Properties": {"custom-avs": "20240101000700-avbooks", "custom-sy-av-view": "v1", "custom-riff-decks": "d"}, "Children": [text("一段")]})]);
    daily["Properties"]["custom-dailynote-20240101"] = json!("20240101");
    let (docs, report) = convert(vec![daily, page("20240101000000-ccccccc", "空", vec![para(vec![text("一段")])])], None);
    for d in &docs { assert_eq!(body(d), "<p>一段</p>\n"); }
    assert_eq!(report.attributes, 0);
    assert_eq!(time("20240102030405"), Some(crate::time::local_to_unix(2024, 1, 2, 3, 4) as u64 + 5));
    assert_eq!(time("20241302030405"), None);
}

#[test]
fn images_and_attachments_are_copied() {
    let dir = crate::vfs::temp_dir().join(format!("qmd-siyuan-assets-{}", uuid::Uuid::new_v4()));
    crate::vfs::create_dir_all(dir.join("av")).unwrap();
    crate::vfs::write(dir.join("图-20240101000000-abcdefg.png"), b"png").unwrap();
    crate::vfs::write(dir.join("说明-20240101000000-hijklmn.pdf"), b"pdf").unwrap();
    let img = |dest: &str| json!({"Type": "NodeImage", "Properties": {"style": "width: 240px"}, "Children": [{"Type": "NodeBang"}, {"Type": "NodeLinkText", "Data": "图"}, {"Type": "NodeLinkDest", "Data": dest}, {"Type": "NodeLinkTitle", "Data": "说明文字"}]});
    let mut file = mark("a", "说明.pdf");
    file["TextMarkAHref"] = json!("assets/说明-20240101000000-hijklmn.pdf");
    let (docs, report) = convert(vec![page("20240101000000-aaaaaaa", "页", vec![
        para(vec![text("前"), img("assets/图-20240101000000-abcdefg.png"), text("后")]),
        para(vec![file]),
        para(vec![img("assets/没有.png")]),
    ])], Some(&dir));
    let _ = crate::vfs::remove_dir_all(&dir);
    let b = &docs[0].blocks;
    assert_eq!((b[0].text.text.as_str(), b[2].text.text.as_str()), ("前", "后"));
    let BlockKind::Image { src, width, caption, .. } = &b[1].kind else { panic!("{:?}", b[1].kind) };
    assert!(src.ends_with("-图.png"), "{src}");
    assert_eq!((*width, caption.as_deref()), (Some(240.), Some("说明文字")));
    let f = b[3].text.runs[0].marks.inline.as_ref().and_then(|i| i.file()).unwrap();
    // `.assets/files/<uuid>/说明.pdf`, with the platform's separator.
    let src = Path::new(&f.src);
    let dirs: Vec<String> = src.ancestors().skip(1).take(2).filter_map(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
    assert!(src.file_name().is_some_and(|n| n == "说明.pdf") && dirs.len() == 2 && dirs[0].len() == 36 && dirs[1] == "files" && f.name == "说明.pdf", "{f:?}");
    assert_eq!(report.missing_assets, ["assets/没有.png"]);
}

#[test]
fn databases_become_static_tables() {
    let dir = crate::vfs::temp_dir().join(format!("qmd-siyuan-av-{}", uuid::Uuid::new_v4()));
    crate::vfs::create_dir_all(dir.join("av")).unwrap();
    crate::vfs::write(dir.join("av").join("20240101000000-avavavv.json"), r#"{"id":"20240101000000-avavavv","name":"书单","keyValues":[{"key":{"id":"k1","name":"书名","type":"block"},"values":[{"blockID":"r1","block":{"content":"三体"}}]}],"views":[{"id":"v","table":{"columns":[{"id":"k1"}],"rowIds":["r1"]}}]}"#).unwrap();
    let (xml, ..) = { let (d, r) = convert(vec![page("20240101000000-aaaaaaa", "页", vec![json!({"Type": "NodeAttributeView", "AttributeViewID": "20240101000000-avavavv", "AttributeViewType": "table"}), json!({"Type": "NodeAttributeView", "AttributeViewID": "20240101000000-missing"})])], Some(&dir)); (body(&d[0]), r) };
    let _ = crate::vfs::remove_dir_all(&dir);
    assert_eq!(xml, "<p><b>书单</b></p>\n<table>\n  <colgroup><col width=\"150\"/></colgroup>\n  <thead>\n    <tr><th><p>书名</p></th></tr>\n  </thead>\n  <tbody>\n    <tr><td><p>三体</p></td></tr>\n  </tbody>\n</table>\n<p><code>20240101000000-missing</code></p>\n");
}

#[test]
fn colors_map_to_the_palette() {
    assert_eq!(color("var(--b3-font-color3)", false), Some(ColorName("blue".into())));
    assert_eq!(color("var(--b3-card-warning-background)", true), Some(ColorName("light-orange".into())));
    assert_eq!(color("#2ea121", false), Some(ColorName("green".into())));
    assert_eq!(color("rgb(128, 128, 130)", true), Some(ColorName("light-gray".into())));
    assert_eq!(color("var(--b3-font-color13)", false), None);
    assert_eq!(asset_name(Path::new("a/图 1-20240101000000-abcdefg.png")), "图 1.png");
    assert_eq!(asset_name(Path::new("a/plain.png")), "plain.png");
    assert_eq!(relative(Path::new("/lib/a/b.xml"), Path::new("/lib/c/d e.xml")), "../c/d%20e.xml");
}

/// The fixture workspace (tests/fixtures/siyuan, made by hand after Lute's
/// node types): every page the import writes, by path, with the copies'
/// random names written alike, then the report.
fn snapshot(lib: &Path, report: &Report) -> String {
    fn files(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in crate::vfs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) { continue }
            if p.fs_is_dir() { files(&p, out) } else if p.extension().is_some_and(|x| x == "xml") { out.push(p) }
        }
    }
    let mut pages = vec![];
    files(lib, &mut pages);
    pages.sort();
    let random = regex::Regex::new(r"\.assets/(?:[0-9a-f]{8}-|files/[0-9a-f-]{36}/)").unwrap();
    let mut out = String::new();
    for p in pages {
        let xml = crate::vfs::read_to_string(&p).unwrap();
        // What the import wrote reads back to the same file.
        assert_eq!(crate::docxml::serialize(&crate::docxml::parse(&xml).unwrap()), xml, "{}", p.display());
        // The library's own files relative to it, with `/` (docs/FORMAT.md).
        assert!(!xml.contains(&*lib.to_string_lossy()) && !xml.contains('\\'), "{}", p.display());
        out.push_str(&format!("==== {}\n{}\n", p.strip_prefix(lib).unwrap().to_string_lossy().replace('\\', "/"), random.replace_all(&xml, ".assets/…/")));
    }
    out.push_str(&format!("==== 报告\n笔记本 {}，文档 {}，失败 {:?}\n降级 {:?}\n块引用 {:?}\n失效引用 {:?}\n嵌入 {}，查询 {}\n数据库 {:?}\n缺失资源 {:?}\n属性 {}\n",
        report.notebooks, report.docs, report.failed, report.degraded, refs(&report.block_refs), refs(&report.broken_refs), report.embeds, report.queries, report.databases, report.missing_assets, report.attributes));
    out
}

/// References as (page, text, target): the page's library id is new each run.
fn refs(r: &[Ref]) -> Vec<(&str, &str, &str)> { r.iter().map(|r| (r.doc.as_str(), r.text.as_str(), r.target.as_str())).collect() }

fn fixtures() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("siyuan") }

fn import_into_new_library(src: &Path) -> (PathBuf, Store, Report) {
    let lib = crate::vfs::temp_dir().join(format!("qmd-siyuan-lib-{}", uuid::Uuid::new_v4()));
    let mut store = Store::open_at(lib.clone()).unwrap();
    store.begin_batch().unwrap();
    let report = import(&mut store, &open(src).unwrap(), None, &Labels::default(), &mut StaticTables).unwrap();
    store.end_batch().unwrap();
    (lib, store, report)
}

#[test]
fn the_fixture_workspace_imports_exactly_as_expected() {
    let (lib, mut store, report) = import_into_new_library(&fixtures().join("data"));
    let got = snapshot(&lib, &report);
    let want = fixtures().join("expected.txt");
    if std::env::var_os("FDL_UPDATE_FIXTURES").is_some() { crate::vfs::write(&want, &got).unwrap(); }
    let entries = store.list();
    let _ = crate::vfs::remove_dir_all(&lib);
    assert_eq!(got, crate::vfs::read_to_string(&want).unwrap(), "FDL_UPDATE_FIXTURES=1 writes the new output");
    // Made when the id says, changed when `updated` does; a child page under its parent.
    let page = |t: &str| entries.iter().find(|e| e.title == t).unwrap_or_else(|| panic!("{t}"));
    assert_eq!(page("随手记").meta.created, time("20240101000400").unwrap());
    assert_eq!(page("随手记").meta.modified, time("20240401120000").unwrap());
    let parent = page("全部块类型");
    assert!(page("子文档").meta.rel.starts_with(&format!("{}/", parent.meta.rel.rsplit_once('/').unwrap().0)), "{:?}", page("子文档").meta.rel);
    assert_eq!(parent.icon.as_deref(), Some("📚"));
}

#[test]
fn a_sy_zip_export_imports_like_its_notebook() {
    // As 导出 › SiYuan .sy.zip lays one out: `<name>/` with the notebook's
    // `.siyuan`, its documents, `assets/` and `storage/av/`.
    let data = fixtures().join("data");
    let nb = data.join("20240101000000-nbnotes");
    let mut entries: Vec<(String, Vec<u8>)> = vec![];
    fn add(dir: &Path, rel: &str, out: &mut Vec<(String, Vec<u8>)>) {
        for e in crate::vfs::read_dir(dir).unwrap().flatten() {
            let (p, name) = (e.path(), e.file_name().to_string_lossy().to_string());
            let r = format!("{rel}/{name}");
            if p.fs_is_dir() { add(&p, &r, out) } else { out.push((r, crate::vfs::read(&p).unwrap())) }
        }
    }
    add(&nb, "读书笔记", &mut entries);
    add(&data.join("assets"), "读书笔记/assets", &mut entries);
    add(&data.join("storage"), "读书笔记/storage", &mut entries);
    let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
    let dir = crate::vfs::temp_dir().join(format!("qmd-siyuan-zip-{}", uuid::Uuid::new_v4()));
    crate::vfs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("读书笔记.sy.zip");
    crate::vfs::write(&zip_path, zip::pack(&refs)).unwrap();
    assert!(recognizes(&zip_path) && recognizes(&data) && recognizes(&nb) && !recognizes(&data.join("assets")));
    assert_eq!(count_docs(&data, 100), 4);
    let (lib, _store, report) = import_into_new_library(&zip_path);
    let got = snapshot(&lib, &report);
    let _ = crate::vfs::remove_dir_all(&lib);
    let _ = crate::vfs::remove_dir_all(&dir);
    let (workspace_lib, _s, _) = import_into_new_library(&data);
    let all = snapshot(&workspace_lib, &Report::default());
    let _ = crate::vfs::remove_dir_all(&workspace_lib);
    // The same pages as the workspace's 读书笔记, and nothing of 收件箱.
    let pages = |s: &str| s.split("==== ").filter(|p| p.starts_with("读书笔记/")).map(str::to_string).collect::<Vec<_>>();
    assert_eq!(pages(&got), pages(&all));
    assert!(!got.contains("收件箱") && report.notebooks == 1 && report.docs == 3);
}

/// `FDL_SIYUAN_DIR=<workspace, notebook or .sy.zip> cargo test --release siyuan_smoke -- --ignored --nocapture`:
/// imports real data into a temporary library, checks every page reads back, and prints the report.
#[test]
#[ignore]
fn siyuan_smoke() {
    let Some(src) = std::env::var_os("FDL_SIYUAN_DIR") else { return };
    let (lib, mut store, report) = import_into_new_library(Path::new(&src));
    let entries = store.list();
    for e in &entries { if let Some(p) = store.file_path(&e.id) && p.extension().is_some_and(|x| x == "xml") { crate::docxml::parse(&crate::vfs::read_to_string(&p).unwrap()).unwrap(); } }
    println!("{} entries\n{report:#?}", entries.len());
    if std::env::var_os("FDL_SIYUAN_KEEP").is_some() { println!("library: {}", lib.display()); return }
    let _ = crate::vfs::remove_dir_all(&lib);
}
