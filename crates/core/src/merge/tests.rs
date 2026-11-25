use super::*;
use crate::docxml::parse;
use crate::model::{BlockKind, RichText};

fn doc(xml: &str) -> Document { parse(xml).unwrap() }
fn xml(d: &Document) -> String { serialize(d) }

fn paras(ps: &[&str]) -> Document {
    doc(&format!("<title>t</title>\n{}", ps.iter().map(|p| format!("<p>{p}</p>\n")).collect::<String>()))
}

fn clean(m: Merge) -> Document {
    match m { Merge::Clean(d) => d, Merge::Conflicts { regions, merged } => panic!("conflicts {regions:?} in\n{}", xml(&merged)) }
}

#[test]
fn edits_to_different_paragraphs_both_land() {
    let base = paras(&["一", "二", "三", "四"]);
    let ours = paras(&["一 改", "二", "三", "四"]);
    let theirs = paras(&["一", "二", "三", "四 改了"]);
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&paras(&["一 改", "二", "三", "四 改了"])));
}

#[test]
fn an_insert_on_one_side_and_an_edit_on_the_other() {
    let base = paras(&["开头", "中间", "结尾"]);
    let ours = paras(&["开头", "新加的一段", "中间", "结尾"]);
    let theirs = paras(&["开头", "中间", "结尾，补一句"]);
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&paras(&["开头", "新加的一段", "中间", "结尾，补一句"])));
}

#[test]
fn both_appending_keeps_both_ours_first() {
    let base = paras(&["一"]);
    let ours = paras(&["一", "Mac 上加的"]);
    let theirs = paras(&["一", "iPad 上加的"]);
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&paras(&["一", "Mac 上加的", "iPad 上加的"])));
}

#[test]
fn the_same_edit_on_both_sides_is_no_conflict() {
    let base = paras(&["一", "二"]);
    let both = paras(&["一", "二 改"]);
    assert_eq!(xml(&clean(merge3(&base, &both, &both))), xml(&both));
}

#[test]
fn both_editing_one_paragraph_keeps_ours_and_says_where() {
    let base = paras(&["一", "今天天气好", "三"]);
    let ours = paras(&["一", "今天上海天气好", "三"]);
    let theirs = paras(&["一", "今天天气好！", "三 改"]);
    let Merge::Conflicts { merged, regions } = merge3(&base, &ours, &theirs) else { panic!() };
    assert_eq!(xml(&merged), xml(&paras(&["一", "今天上海天气好", "三 改"])));
    assert_eq!(regions, vec![Conflict::Blocks { parent: vec![], start: 1, len: 1 }]);
}

#[test]
fn removing_what_the_other_side_edited_is_a_conflict() {
    let base = paras(&["一", "二", "三"]);
    let ours = paras(&["一", "三"]);
    let theirs = paras(&["一", "二 改过", "三"]);
    let Merge::Conflicts { merged, regions } = merge3(&base, &ours, &theirs) else { panic!() };
    assert_eq!(xml(&merged), xml(&ours));
    assert_eq!(regions, vec![Conflict::Blocks { parent: vec![], start: 1, len: 0 }]);
}

#[test]
fn removing_an_untouched_paragraph_is_taken() {
    let base = paras(&["一", "二", "三"]);
    let ours = paras(&["一", "二", "三 改"]);
    let theirs = paras(&["一", "三"]);
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&paras(&["一", "三 改"])));
}

#[test]
fn edits_inside_one_container_merge_child_by_child() {
    let base = doc("<title>t</title>\n<blockquote><p>甲</p><p>乙</p></blockquote>\n<callout emoji=\"💡\"><p>丙</p></callout>\n");
    let ours = doc("<title>t</title>\n<blockquote><p>甲 改</p><p>乙</p></blockquote>\n<callout emoji=\"💡\"><p>丙</p></callout>\n");
    let theirs = doc("<title>t</title>\n<blockquote><p>甲</p><p>乙 改</p></blockquote>\n<callout emoji=\"🔥\"><p>丙</p></callout>\n");
    let want = doc("<title>t</title>\n<blockquote><p>甲 改</p><p>乙 改</p></blockquote>\n<callout emoji=\"🔥\"><p>丙</p></callout>\n");
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&want));
}

#[test]
fn a_list_items_text_and_its_children_merge_apart() {
    let base = doc("<title>t</title>\n<ul><li>项<ul><li>子一</li></ul></li></ul>\n");
    let ours = doc("<title>t</title>\n<ul><li>项 改<ul><li>子一</li></ul></li></ul>\n");
    let theirs = doc("<title>t</title>\n<ul><li>项<ul><li>子一</li><li>子二</li></ul></li></ul>\n");
    let want = doc("<title>t</title>\n<ul><li>项 改<ul><li>子一</li><li>子二</li></ul></li></ul>\n");
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&want));
}

#[test]
fn a_nested_conflict_points_into_its_parent() {
    let base = doc("<title>t</title>\n<p>前</p>\n<blockquote><p>甲</p></blockquote>\n");
    let ours = doc("<title>t</title>\n<p>前</p>\n<blockquote><p>甲 我</p></blockquote>\n");
    let theirs = doc("<title>t</title>\n<p>前</p>\n<blockquote><p>甲 你</p></blockquote>\n");
    let Merge::Conflicts { regions, .. } = merge3(&base, &ours, &theirs) else { panic!() };
    assert_eq!(regions, vec![Conflict::Blocks { parent: vec![1], start: 0, len: 1 }]);
}

#[test]
fn a_head_that_takes_no_children_keeps_ours_whole() {
    let base = doc("<title>t</title>\n<p>甲</p>\n");
    let ours = doc("<title>t</title>\n<pre lang=\"text\"><code>甲</code></pre>\n");
    let theirs = doc("<title>t</title>\n<p>甲<p>缩进的</p></p>\n");
    let m = merge3(&base, &ours, &theirs);
    assert!(matches!(m, Merge::Conflicts { .. }));
    assert_eq!(xml(m.document()), xml(&ours));
}

#[test]
fn title_icon_and_cover_merge_field_by_field() {
    let base = doc("<title>旧</title>\n<p>x</p>\n");
    let ours = doc("<title>新</title>\n<p>x</p>\n");
    let theirs = doc("<title icon=\"📌\">旧</title>\n<p>x</p>\n");
    assert_eq!(xml(&clean(merge3(&base, &ours, &theirs))), xml(&doc("<title icon=\"📌\">新</title>\n<p>x</p>\n")));
    let other = doc("<title>另一个</title>\n<p>x</p>\n");
    let Merge::Conflicts { merged, regions } = merge3(&base, &ours, &other) else { panic!() };
    assert_eq!((merged.title.text.as_str(), regions), ("新", vec![Conflict::Title]));
}

#[test]
fn everything_removed_leaves_one_empty_paragraph() {
    let base = paras(&["一", "二"]);
    let ours = paras(&["一"]);
    let mut theirs = paras(&["二"]);
    theirs.blocks[0] = Block::new(BlockKind::Text, RichText::plain("二"));
    let m = merge3(&base, &ours, &theirs);
    assert_eq!(m.document().blocks.len(), 1);
}

#[test]
fn moving_a_paragraph_the_other_side_edited_is_a_conflict_not_a_loss() {
    // Without stable block ids a move reads as a removal plus an insertion
    // (sync.md §3.4): the merge keeps ours and says so; theirs is kept apart.
    let base = paras(&["一", "二", "三", "四"]);
    let ours = paras(&["二", "三", "四", "一"]);
    let theirs = paras(&["一 改", "二", "三", "四"]);
    let m = merge3(&base, &ours, &theirs);
    assert!(matches!(m, Merge::Conflicts { .. }));
    assert_eq!(xml(m.document()), xml(&ours));
}

#[test]
fn a_long_document_merges_quickly() {
    let n = 5000;
    let base: Vec<String> = (0..n).map(|i| format!("第{i}段")).collect();
    let (mut ours, mut theirs) = (base.clone(), base.clone());
    for i in (0..n).step_by(10) { ours[i].push_str(" 我") }
    for i in (5..n).step_by(10) { theirs[i].push_str(" 你") }
    let d = |ps: &[String]| paras(&ps.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let t = std::time::Instant::now();
    let m = clean(merge3(&d(&base), &d(&ours), &d(&theirs)));
    assert_eq!(m.blocks.len(), n);
    assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
}

// ------------------------------------------------------------ random merges

pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    pub(crate) fn below(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize }
    fn chance(&mut self, percent: u64) -> bool { self.next() % 100 < percent }
    fn word(&mut self) -> String { (0..6).map(|_| char::from(b'a' + self.below(26) as u8)).collect() }
}

fn seeds() -> u64 { std::env::var("FDL_FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(200) }

pub(crate) const NESTED: &str = r#"<title>随机 <b>合并</b></title>
<h1>标题一</h1>
<p>普通 <b>加粗</b> <em>斜体</em> 😀</p>
<ul><li>一<ul><li>一.一</li><li>一.二</li></ul></li><li>二</li></ul>
<ol><li>三</li><li>四</li></ol>
<checkbox done="true">完成</checkbox>
<blockquote><p>引用一</p><p>引用二</p></blockquote>
<pre lang="rust"><code>fn main() {}</code></pre>
<hr/>
<callout emoji="💡"><p>提示</p><ul><li>提示列表</li></ul></callout>
<grid><column width-ratio="0.5"><p>左</p></column><column width-ratio="0.5"><p>右</p><p>右二</p></column></grid>
<p></p>
<p>结尾</p>
"#;

/// Every (path, block) in the document, parents before children.
fn paths(blocks: &[Block], at: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    for (i, b) in blocks.iter().enumerate() {
        at.push(i);
        out.push(at.clone());
        paths(&b.children, at, out);
        at.pop();
    }
}

fn list_at<'a>(blocks: &'a mut Vec<Block>, parent: &[usize]) -> &'a mut Vec<Block> {
    match parent.split_first() { None => blocks, Some((&i, rest)) => list_at(&mut blocks[i].children, rest) }
}

fn holds_text(k: &BlockKind) -> bool { k.nests() || matches!(k, BlockKind::Code { .. }) }

/// One random edit anywhere in the document, as a user could make it.
fn mutate(d: &mut Document, rng: &mut Rng) {
    let mut all = vec![];
    paths(&d.blocks, &mut vec![], &mut all);
    let p = all[rng.below(all.len())].clone();
    let (parent, i) = (p[..p.len() - 1].to_vec(), p[p.len() - 1]);
    let list = list_at(&mut d.blocks, &parent);
    let takes_text = |l: &Vec<Block>| parent.is_empty() || !matches!(l.first().map(|b| &b.kind), Some(BlockKind::Column { .. }));
    match rng.below(4) {
        0 | 1 if holds_text(&list[i].kind) => {
            let w = rng.word();
            let b = &mut list[i];
            b.text = if rng.chance(50) { RichText::plain(format!("{} {w}", b.text.plain_text())) } else { RichText::plain(w) };
        }
        2 if takes_text(list) => { let at = rng.below(list.len() + 1); list.insert(at, Block::text(&rng.word())) }
        3 if !matches!(list[i].kind, BlockKind::Column { .. }) && list.len() > 1 => { list.remove(i); }
        _ => {}
    }
}

fn valid(blocks: &[Block]) -> bool {
    blocks.iter().all(|b| (b.children.is_empty() || b.kind.nests() || b.kind.is_container()) && valid(&b.children))
}

pub(crate) fn edited(base: &Document, rng: &mut Rng) -> Document {
    let mut d = base.clone();
    for _ in 0..rng.below(4) { mutate(&mut d, rng) }
    d
}

#[test]
fn random_merges_are_whole_and_well_formed() {
    let base0 = doc(NESTED);
    for seed in 1..=seeds() {
        let mut rng = Rng(seed * 0x9e37_79b9 + 7);
        let base = edited(&base0, &mut rng);
        let (ours, theirs) = (edited(&base, &mut rng), edited(&base, &mut rng));
        let m = merge3(&base, &ours, &theirs);
        let out = xml(m.document());
        assert!(valid(&m.document().blocks), "seed {seed}: children where none may be\n{out}");
        assert_eq!(xml(&parse(&out).unwrap()), out, "seed {seed}: does not round-trip");
        // One side unchanged: the other side, exactly.
        assert_eq!(xml(&clean(merge3(&base, &ours, &base))), xml(&ours), "seed {seed}: theirs unchanged");
        assert_eq!(xml(&clean(merge3(&base, &base, &theirs))), xml(&theirs), "seed {seed}: ours unchanged");
        assert_eq!(xml(&clean(merge3(&base, &ours, &ours))), xml(&ours), "seed {seed}: same on both");
    }
}

/// What one side did to a paragraph of the base, by its index there.
#[derive(Clone)]
enum Op { Edit(String), Rewrite(String), Delete, InsertAfter(String) }

/// Edits that touch paragraphs at least three apart on the two sides: the
/// merge must be clean and equal to applying both.
#[test]
fn random_disjoint_edits_merge_to_both_applied() {
    for seed in 1..=seeds() {
        let mut rng = Rng(seed * 0x2545_f491 + 3);
        let n = 4 + rng.below(20);
        let base: Vec<String> = (0..n).map(|i| format!("第{i}段 {}", rng.word())).collect();
        let mut ops: Vec<Vec<(usize, Op)>> = vec![vec![], vec![]];
        let mut touched: Vec<(usize, usize)> = vec![];
        for _ in 0..rng.below(8) {
            let side = rng.below(2);
            let i = rng.below(n);
            if touched.iter().any(|&(s, j)| s != side && i.abs_diff(j) < 3) || ops[side].iter().any(|(j, _)| *j == i) { continue }
            let w = rng.word();
            let op = match rng.below(4) { 0 => Op::Edit(format!("{} {w}", base[i])), 1 => Op::Rewrite(format!("{w}{w}")), 2 => Op::Delete, _ => Op::InsertAfter(format!("新 {w}")) };
            touched.push((side, i));
            ops[side].push((i, op));
        }
        let apply = |sides: &[&Vec<(usize, Op)>]| -> Vec<String> {
            let mut out = vec![];
            for (i, p) in base.iter().enumerate() {
                let op = sides.iter().find_map(|s| s.iter().find(|(j, _)| *j == i).map(|(_, o)| o.clone()));
                match op {
                    None => out.push(p.clone()),
                    Some(Op::Edit(t) | Op::Rewrite(t)) => out.push(t),
                    Some(Op::Delete) => {}
                    Some(Op::InsertAfter(t)) => { out.push(p.clone()); out.push(t) }
                }
            }
            out
        };
        let d = |ps: Vec<String>| paras(&ps.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let (b, o, t, want) = (d(base.clone()), d(apply(&[&ops[0]])), d(apply(&[&ops[1]])), d(apply(&[&ops[0], &ops[1]])));
        assert_eq!(xml(&clean(merge3(&b, &o, &t))), xml(&want), "seed {seed}");
    }
}
