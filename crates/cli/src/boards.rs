//! 画板 from the CLI: a board file, or a board block in a document. Read as
//! a list of elements; written with Excalidraw element skeletons (what
//! `convertToExcalidrawElements` takes: `{type: "rectangle", x, y, label:
//! {text}}`, arrows with `start` / `end`), appended or replacing the scene,
//! or by removing elements by id.

use fdl_core::board::binding::{NO_BINDING_HOOKS, bind_binding_element};
use fdl_core::board::consts::DEFAULT_ZOOM;
use fdl_core::board::fractional_index::sync_invalid_indices;
use fdl_core::board::transform::{TransformOpts, convert_to_excalidraw_elements};
use fdl_core::board::types::{BindMode, Element, StartOrEnd};
use fdl_core::model::{Block, BlockKind};
use fdl_core::whiteboard::{ApproxMeasure, Board};
use serde_json::{Value, json};

use crate::blocks;
use crate::fail::{R, fail};
use crate::session::{Session, arg, flag};

fn centre(b: (f64, f64, f64, f64)) -> (f64, f64) { (b.0 + b.2 / 2., b.1 + b.3 / 2.) }

/// Where a line from the middle of box `b` toward `to` leaves it, a little outside.
fn edge(b: (f64, f64, f64, f64), to: (f64, f64)) -> (f64, f64) {
    let c = centre(b);
    let (dx, dy) = (to.0 - c.0, to.1 - c.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let t = (if dx.abs() > 1e-9 { b.2 / 2. / dx.abs() } else { f64::INFINITY }).min(if dy.abs() > 1e-9 { b.3 / 2. / dy.abs() } else { f64::INFINITY });
    let t = if t.is_finite() { t } else { 0. };
    (c.0 + dx * t + dx / len * 8., c.1 + dy * t + dy / len * 8.)
}

fn round(x: f64) -> Value { let r = (x * 10.).round() / 10.; if r.fract() == 0. && r.abs() < 1e15 { json!(r as i64) } else { json!(r) } }

/// One element as an agent needs it: what it is, where, and what it says.
fn element_json(e: &Element, all: &[Element]) -> Value {
    let mut v = json!({ "id": e.id, "type": e.kind.element_type().as_str(), "x": round(e.x), "y": round(e.y), "w": round(e.width), "h": round(e.height) });
    if let Some(t) = e.text() { v["text"] = json!(t.original_text); }
    if let Some(c) = e.container_id() { v["in"] = json!(c); }
    // A container's label is its bound text element.
    if let Some(label) = e.bound_elements.iter().flatten().find_map(|b| all.iter().find(|x| x.id == b.id && x.container_id() == Some(e.id.as_str()))).and_then(|x| x.text()) { v["label"] = json!(label.original_text); }
    if let Some(l) = e.linear() {
        if let Some(b) = &l.start_binding { v["from"] = json!(b.element_id); }
        if let Some(b) = &l.end_binding { v["to"] = json!(b.element_id); }
    }
    if let Some(n) = e.frame_name() { v["name"] = json!(n); }
    if let Some(f) = &e.frame_id { v["frame"] = json!(f); }
    if !e.background_color.is_empty() && e.background_color != "transparent" { v["fill"] = json!(e.background_color); }
    if let Some(l) = &e.link { v["link"] = json!(l); }
    v
}

impl Session {
    /// The block holding the board: the document's only block for a board
    /// file, else the board block `at`.
    fn board_block(&self, i: usize, a: &Value) -> R<fdl_core::model::BlockId> {
        let doc = &self.docs[i].doc;
        if let Some(x) = arg(a, "at") {
            let id = blocks::resolve(doc, x)?;
            return match &blocks::find(doc, id).unwrap().kind { BlockKind::Board(_) => Ok(id), k => Err(fail("bad_kind", format!("{x} is a {} block, not a board", blocks::kind_name(k)))) };
        }
        blocks::items(doc).iter().find(|it| matches!(it.block.kind, BlockKind::Board(_))).map(|it| it.block.id)
            .ok_or_else(|| fail("bad_kind", format!("{} holds no board", self.docs[i].path)).hint("pass --at <ref> of a board block"))
    }

    fn board_of(&self, i: usize, a: &Value) -> R<(fdl_core::model::BlockId, Board)> {
        let id = self.board_block(i, a)?;
        match &blocks::find(&self.docs[i].doc, id).unwrap().kind { BlockKind::Board(b) => Ok((id, (**b).clone())), _ => unreachable!() }
    }

    fn put_board(&mut self, i: usize, id: fdl_core::model::BlockId, board: Board) {
        fn walk(list: &mut [Block], id: fdl_core::model::BlockId, board: &mut Option<Board>) {
            for b in list {
                if b.id == id { if let (BlockKind::Board(bd), Some(new)) = (&mut b.kind, board.take()) { *bd = std::sync::Arc::new(new); } return; }
                walk(&mut b.children, id, board);
            }
        }
        let d = &mut self.docs[i];
        let before = d.doc.clone();
        walk(&mut d.doc.blocks, id, &mut Some(board));
        if d.doc != before { d.dirty = true; }
    }

    /// Elements in z-order, and the box they fill (`full`: the scene as Excalidraw JSON too).
    pub(crate) fn board(&mut self, a: &Value) -> R<Value> {
        let i = self.doc_arg(a)?;
        let (_, bd) = self.board_of(i, a)?;
        let els: Vec<Value> = bd.elements.iter().map(|e| element_json(e, &bd.elements)).collect();
        let mut v = json!({ "path": self.docs[i].path, "rev": self.docs[i].rev, "elements": els });
        if let Some((x0, y0, x1, y1)) = bd.elements.iter().map(|e| (e.x, e.y, e.x + e.width, e.y + e.height)).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))) {
            v["bounds"] = json!({ "x": round(x0), "y": round(y0), "w": round(x1 - x0), "h": round(y1 - y0) });
        }
        if flag(a, "full") { v["scene"] = serde_json::from_str(&bd.file_json()).unwrap_or(Value::Null); }
        Ok(v)
    }

    /// Add elements from skeletons (`replace`: instead of what is there).
    pub(crate) fn board_add(&mut self, a: &Value) -> R<Value> {
        let i = self.doc_arg(a)?;
        let (id, mut bd) = self.board_of(i, a)?;
        let skeletons = match a.get("elements") {
            Some(v @ Value::Array(_)) => v.clone(),
            Some(Value::Object(o)) if o.get("elements").is_some_and(Value::is_array) => o["elements"].clone(),
            _ => return Err(fail("bad_args", "elements: a list of Excalidraw element skeletons ({type, x, y, …})")),
        };
        let convert = |sk: &Value| convert_to_excalidraw_elements(sk, &TransformOpts { regenerate_ids: false, measure: &ApproxMeasure, hooks: &NO_BINDING_HOOKS })
            .map_err(|e| fail("bad_args", format!("elements: {e}")).hint("each needs a type (rectangle, ellipse, diamond, text, arrow, line, frame…) and x, y; see `quiet help board`"));
        if flag(a, "replace") { bd.elements.clear(); }
        // Arrows and lines between elements named by id (new or already on
        // the board) are drawn after the shapes, from edge to edge when they
        // have no position of their own, then bound as the app binds them.
        let ends = |s: &Value, k: &str| s.get(k).and_then(|e| e.get("id")).and_then(Value::as_str).map(str::to_string);
        let joins = |s: &Value| matches!(s.get("type").and_then(Value::as_str), Some("arrow" | "line")) && (ends(s, "start").is_some() || ends(s, "end").is_some());
        let list = skeletons.as_array().cloned().unwrap_or_default();
        let (links, shapes): (Vec<Value>, Vec<Value>) = list.into_iter().partition(|s| joins(s));
        let mut made = convert(&json!(shapes))?;
        let mut bindings = vec![];
        for mut sk in links {
            let (from, to) = (ends(&sk, "start"), ends(&sk, "end"));
            let boxed = |id: &Option<String>| -> R<Option<(f64, f64, f64, f64)>> {
                let Some(id) = id else { return Ok(None) };
                bd.elements.iter().chain(&made).find(|e| &e.id == id).map(|e| Some((e.x, e.y, e.width, e.height)))
                    .ok_or_else(|| fail("not_found", format!("an arrow ends at {id:?}, and no element has that id")).hint("give the shape an \"id\" in the same list, or use one `quiet board` lists"))
            };
            let (b1, b2) = (boxed(&from)?, boxed(&to)?);
            let o = sk.as_object_mut().unwrap();
            o.remove("start");
            o.remove("end");
            if !o.contains_key("x") || !o.contains_key("y") {
                let (Some(b1), Some(b2)) = (b1, b2) else { return Err(fail("bad_args", "an arrow with one bound end needs its x and y")) };
                let (p, q) = (edge(b1, centre(b2)), edge(b2, centre(b1)));
                o.insert("x".into(), json!(p.0));
                o.insert("y".into(), json!(p.1));
                o.insert("width".into(), json!(q.0 - p.0));
                o.insert("height".into(), json!(q.1 - p.1));
                o.insert("points".into(), json!([[0., 0.], [q.0 - p.0, q.1 - p.1]]));
            }
            let els = convert(&json!([sk]))?;
            if let Some(arrow) = els.iter().find(|e| e.linear().is_some()) { bindings.push((arrow.id.clone(), from, to)); }
            made.extend(els);
        }
        if made.is_empty() { return Err(fail("bad_args", "no elements were made from that")); }
        // Labels come with their shapes; only the shapes are named back.
        let new_ids: Vec<String> = made.iter().filter(|e| e.container_id().is_none()).map(|e| e.id.clone()).collect();
        if let Some(dup) = made.iter().find(|e| bd.elements.iter().any(|x| x.id == e.id)) { return Err(fail("bad_args", format!("an element with id {:?} is already on the board", dup.id)).hint("leave id out, or pick another")); }
        bd.elements.extend(made);
        for (arrow, from, to) in bindings {
            for (end, side) in [(from, StartOrEnd::Start), (to, StartOrEnd::End)] {
                if let Some(target) = end { bind_binding_element(&arrow, &target, BindMode::Orbit, side, &mut bd.elements, DEFAULT_ZOOM, None, true, true); }
            }
        }
        sync_invalid_indices(&mut bd.elements);
        self.put_board(i, id, bd);
        Ok(json!({ "path": self.docs[i].path, "added": new_ids }))
    }

    /// Remove elements by id, with the labels bound to them.
    pub(crate) fn board_remove(&mut self, a: &Value) -> R<Value> {
        let i = self.doc_arg(a)?;
        let (id, mut bd) = self.board_of(i, a)?;
        let ids: Vec<String> = match a.get("ids") { Some(Value::Array(l)) => l.iter().filter_map(Value::as_str).map(str::to_string).collect(), Some(Value::String(s)) => s.split([',', ' ']).filter(|s| !s.is_empty()).map(str::to_string).collect(), _ => return Err(fail("bad_args", "ids: the elements to remove")) };
        if let Some(x) = ids.iter().find(|x| !bd.elements.iter().any(|e| &e.id == *x)) { return Err(fail("not_found", format!("no element {x:?} on the board")).hint("`quiet board DOC` lists them")); }
        let n = bd.elements.len();
        bd.elements.retain(|e| !ids.contains(&e.id) && !e.container_id().is_some_and(|c| ids.iter().any(|x| x == c)));
        let removed = n - bd.elements.len();
        for e in &mut bd.elements { if let Some(list) = &mut e.bound_elements { list.retain(|b| !ids.contains(&b.id)); } }
        self.put_board(i, id, bd);
        Ok(json!({ "path": self.docs[i].path, "removed": removed }))
    }
}
