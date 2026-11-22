//! Images: the image tool's file picker, files dropped on the canvas and
//! pasted pictures (scene files dropped too), after Excalidraw `App.tsx`'s `onImageToolbarButtonClick`,
//! `insertImages` and `handleAppOnDrop` @ 3a317b5. Files are copied into
//! the library's `.assets` as document images are (`editor::image`), and
//! the scene's `files` keep their library-relative path.

use super::super::BoardView;
use super::super::clipboard::{DropContent, DroppedFile, route_drop};
use super::super::history::CaptureUpdateAction;
use super::super::image::{generate_id_from_file, initialize_image_element, new_image_placeholder, normalize_file_mime_type, position_elements_on_grid};
use super::super::types::{FileData, ImageStatus};
use gpui::{Context, PathPromptOptions, Window};
use std::path::PathBuf;

/// The image tool: pick files, insert them at the view's center.
pub fn pick_images(me: &mut BoardView, window: &mut Window, cx: &mut Context<BoardView>) {
    let picked = crate::devtools::take_pick();
    let rx = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(crate::settings::text("插入图片").into()), #[cfg(not(target_os = "ios"))] extensions: None }));
    let _ = me;
    cx.spawn_in(window, async move |this, cx| {
        let paths = match rx { Some(rx) => rx.await.ok().and_then(Result::ok).flatten(), None => picked };
        let _ = this.update_in(cx, |me, window, cx| {
            let c = me.canvas();
            let at = me.scene_point(gpui::point(gpui::px((c.offset_left + c.width / 2.) as f32), gpui::px((c.offset_top + c.height / 2.) as f32)));
            insert_files(me, paths.unwrap_or_default(), at, window, cx);
        });
    }).detach();
}

/// Files dropped on the canvas (`handleAppOnDrop` → core's `route_drop`):
/// a `.excalidraw` file, or a lone PNG / SVG with a scene in it, puts its
/// elements in where it fell (upstream replaces the scene with a dropped
/// file; inside a document we add to it instead); images go in as images.
pub fn drop_files(me: &mut BoardView, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<BoardView>) {
    let at = me.scene_point(me.pointer);
    let read: Vec<(String, Vec<u8>)> = paths.iter().filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), fdl_core::vfs::read(p).ok()?))).collect();
    let files: Vec<DroppedFile> = read.iter().map(|(n, b)| DroppedFile { name: n, mime_type: "", bytes: b }).collect();
    match route_drop(&files, Some(&me.scene)) {
        DropContent::Scene(s) | DropContent::SceneFile(s) => {
            me.finish_pending(window, cx);
            super::super::run::insert_scene(me, &s.elements, Some(&s.files), at, true, cx);
        }
        DropContent::Images(ix) => {
            let picked: Vec<PathBuf> = ix.into_iter().filter_map(|i| paths.get(i).cloned()).collect();
            insert_files(me, picked, at, window, cx);
        }
        DropContent::Error(_) | DropContent::Nothing => {}
    }
}

/// Reads, stores and inserts image files (others are skipped).
pub fn insert_files(me: &mut BoardView, paths: Vec<PathBuf>, at: [f64; 2], window: &mut Window, cx: &mut Context<BoardView>) {
    let images: Vec<(Vec<u8>, String, Option<String>)> = paths.iter().filter_map(|p| {
        let ext = p.extension()?.to_str()?.to_ascii_lowercase();
        let bytes = fdl_core::vfs::read(p).ok()?;
        Some((bytes, ext, p.file_stem().and_then(|s| s.to_str()).map(str::to_owned)))
    }).collect();
    insert_images(me, images, at, window, cx);
}

/// `insertImages`: (bytes, extension, name) laid out on a grid at `at`.
pub fn insert_images(me: &mut BoardView, images: Vec<(Vec<u8>, String, Option<String>)>, at: [f64; 2], window: &mut Window, cx: &mut Context<BoardView>) {
    me.finish_pending(window, cx);
    let mut units = vec![];
    for (bytes, ext, name) in images {
        let Ok(path) = crate::editor::image::store_image_named(&me.library_root, &bytes, &ext, name.as_deref()) else { continue };
        let rel = path.strip_prefix(&me.library_root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| path.to_string_lossy().into_owned());
        let id = generate_id_from_file(&bytes);
        let mime = normalize_file_mime_type(&format!("x.{ext}"), "", &bytes);
        let natural = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok()).map(|(w, h)| (w as f64, h as f64));
        let now = super::super::mutate::get_updated_timestamp();
        me.files.entry(id.clone()).or_insert(FileData { id: id.clone(), mime_type: mime, data_url: None, path: Some(rel), created: now, last_retrieved: Some(now), version: None, extra: Default::default() });
        let frame = me.top_frame_at(at, &Default::default());
        let ph = new_image_placeholder(&me.app, at[0], at[1], me.grid(Default::default()), frame);
        let mut e = initialize_image_element(&ph, &id, natural, me.canvas().height, me.app.zoom);
        if let Some(i) = e.image_mut() { i.status = ImageStatus::Saved; }
        units.push(vec![e]);
    }
    if units.is_empty() {
        me.app.active_tool = super::super::actions::update_active_tool(&me.app, super::super::app_state::ToolType::Selection);
        cx.notify();
        return;
    }
    let placed = position_elements_on_grid(&units, at[0], at[1], 50. / me.app.zoom);
    let ids: Vec<String> = placed.iter().map(|e| e.id.clone()).collect();
    me.scene.insert_elements_at_index(placed, None);
    super::selection::select_only(me, &ids, false);
    me.app.active_tool = super::super::actions::update_active_tool(&me.app, super::super::app_state::ToolType::Selection);
    me.commit(Some(CaptureUpdateAction::Immediately), cx);
}
