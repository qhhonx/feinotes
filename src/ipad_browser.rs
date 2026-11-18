//! The iPad browser remains independent of the live document view.
use crate::{app::{Workspace,View}, settings::{self,tr,display_title}};
use gpui::*;
use std::{cell::RefCell,collections::HashMap};

thread_local! { static QUERIES: RefCell<HashMap<String,String>> = RefCell::new(HashMap::new()); }

pub(crate) fn scope(w: &Workspace,cx: &App) -> String { format!("{:?}|{:?}|{:?}|{:?}",if w.mobile_personal_browser(cx) { crate::store::Store::root_dir() } else { w.store.root().to_path_buf() },w.folder_current,w.home.tab,w.sharing.collection) }

fn entries(w: &Workspace,cx: &App) -> Vec<crate::store::Entry> { w.mobile_visible_entries(cx) }

pub(crate) fn project(w: &Workspace,cx: &App) -> Option<serde_json::Value> {
    if unsafe { crate::ipad::fei_ios_is_phone() } { return None; }
    let personal = !w.is_shared() || w.mobile_personal_browser(cx);
    let shared = personal && w.home.tab==crate::home::HomeTab::Shared;
    let mut path = if shared { w.mobile_collection_path(cx) } else { vec![] };
    if path.is_empty() && w.folder_current.is_some() {
        let personal_entries = (personal && w.is_shared()).then(||w.personal_navigation_entries(cx));
        let entries = personal_entries.as_deref().unwrap_or(&w.entries);
        let mut folder = w.folder_current.clone();
        while let Some(id) = folder {
            if path.iter().any(|(seen,_)|seen==&id) { break; }
            let Some(entry) = entries.iter().find(|entry|entry.id==id && entry.meta.folder.is_some()) else { break; };
            path.push((id,display_title(&entry.title)));
            folder = entry.meta.parent.clone();
        }
        path.reverse();
    }
    let folder = path.last().map(|(id,_)|id.clone());
    let title = path.last().map(|(_,title)|title.clone()).unwrap_or_else(||tr("主页","Home").into());
    let path = path.into_iter().map(|(id,title)|serde_json::json!({"key":format!("folder:{id}"),"title":title})).collect::<Vec<_>>();
    let query = w.query(cx);
    let rows = entries(w,cx).into_iter().map(|entry|serde_json::json!({"id":entry.id,"title":display_title(&entry.title),"folder":entry.meta.folder.is_some(),"pinned":entry.meta.pinned,"reorderable":w.mobile_row_reorderable(&entry,cx),
        "summary":if entry.meta.folder.is_some() { String::new() } else { crate::mobile::list_summary(&entry.body,&query) }})).collect::<Vec<_>>();
    Some(serde_json::json!({"folder":folder,"favorites":w.home.tab==crate::home::HomeTab::Starred,"shared":shared,"personal":personal,"title":title,"path":path,
        "listing":{"kind":if shared { "shared" } else { "notes" },"can_select":w.mobile_can_select(cx),"can_sort":w.mobile_can_sort(cx),"can_reorder":w.mobile_can_reorder(cx),"selection":w.mobile_batch_state(cx),"shared_count":w.shared_entries(cx).len(),"notice_kind":w.toast.as_ref().map(|(kind,_,_)|match kind { crate::ui::ToastKind::Success=>"success",crate::ui::ToastKind::Error=>"error",crate::ui::ToastKind::Info=>"info" }),"interface_size":settings::prefs().ui_font,"notice":w.toast.as_ref().filter(|(_,_,time)|time.elapsed().as_secs_f32()<3.).map(|(_,text,_)|text.to_string()),"scope":scope(w,cx),"query":w.search.read(cx).text,"tab":match w.home.tab { crate::home::HomeTab::Starred=>1,crate::home::HomeTab::Shared=>2,_=>0 },"tabs":folder.is_none() && query.is_empty() && personal,
        "empty":if !query.is_empty() { tr("没有找到匹配的结果","No matching documents") } else if w.home.tab==crate::home::HomeTab::Starred { tr("暂无收藏文档","No favorite documents yet") } else if w.home.tab==crate::home::HomeTab::Shared { tr("暂无共享文档。你可以从菜单接受邀请，或邀请他人一起编辑文档。","No shared documents yet. Accept an invitation from the menu, or invite someone to edit a document with you.") } else { tr("点新建按钮创建文档","Tap New to start a document") },"rows":rows}}))
}

fn select_scope(w: &mut Workspace,target: String,window: &mut Window,cx: &mut Context<Workspace>) {
    let folder = target.strip_prefix("folder:").map(str::to_owned);
    if let Some(id) = &folder {
        if !w.mobile_collection_exists(id,cx) && !w.entries.iter().chain(w.personal_navigation_entries(cx).iter()).any(|e|&e.id==id&&e.meta.folder.is_some()) { return; }
    } else if !matches!(target.as_str(),"all"|"favorites"|"shared") { return; }
    let keep_editor = w.mobile_personal_browser(cx);
    if w.is_shared() && !keep_editor && folder.as_ref().is_none_or(|id|!w.entries.iter().any(|e|&e.id==id)) {
        w.personal_action(window,cx,move|w,window,cx|select_scope(w,target,window,cx)); return;
    }
    let retry = target.clone();
    if !keep_editor && !w.save_before(cx,move|w,window,cx|select_scope(w,retry,window,cx)) { return; }
    QUERIES.with(|queries|queries.borrow_mut().insert(scope(w,cx),w.search.read(cx).text.clone()));
    w.mobile_batch=None; w.batch=None;
    let (collection,folder) = if folder.as_ref().is_some_and(|id|w.mobile_collection_exists(id,cx)) { (folder,None) } else { (None,folder) };
    w.home.tab = if target=="shared" || collection.is_some() { crate::home::HomeTab::Shared } else if target=="favorites" { crate::home::HomeTab::Starred } else { crate::home::HomeTab::Recent };
    w.sharing.collection=collection;
    w.folder_current=folder;
    let query=QUERIES.with(|queries|queries.borrow().get(&scope(w,cx)).cloned().unwrap_or_default());
    w.search.update(cx,|field,cx|field.set_text(&query,cx));
    if keep_editor {
        w.remember_personal_browser(cx);
        cx.notify();
    } else {
        w.mobile_navigation.links.clear(); w.mobile_navigation.trash=None;
        w.show_view(if w.folder_current.is_some(){View::Folder}else{View::Home},cx);
    }
}

pub(crate) fn command(w:&mut Workspace,action:&str,value:Option<&str>,id:Option<&str>,expected:Option<&str>,window:&mut Window,cx:&mut Context<Workspace>) {
    if unsafe { crate::ipad::fei_ios_is_phone() } || w.library_switching || w.store.file_task_busy() || w.file_history.busy
        || w.confirm.is_some() || expected!=Some(scope(w,cx).as_str()) { return; }
    match action {
        "browse-scope"=>if let Some(value)=value { select_scope(w,value.into(),window,cx); },
        "browse-search"=>if let Some(value)=value {
            w.search.update(cx,|field,cx|field.set_text(value,cx));
            if w.mobile_personal_browser(cx) { w.remember_personal_browser(cx); }
            cx.notify();
        },
        "browse-open"=>if let Some(id)=id { if let Some(entry)=entries(w,cx).iter().find(|e|e.id==id) { if entry.meta.folder.is_some() && w.shared_list_room(id,cx).is_none() { select_scope(w,format!("folder:{id}"),window,cx); } else { w.mobile_open_entry(id,window,cx); } } },
        "browse-new-folder"=>if let Some(name)=value.map(str::trim).filter(|v|!v.is_empty()) {
            if (!w.is_shared() || w.mobile_personal_browser(cx)) && w.home.shared() { return; }
            if w.mobile_personal_browser(cx) {
                let name=name.to_owned(); let id=id.map(str::to_owned);
                w.personal_action(window,cx,move|w,window,cx|{ let expected=scope(w,cx); command(w,"browse-new-folder",Some(&name),id.as_deref(),Some(&expected),window,cx); });
                return;
            }
            let parent=id.map(str::to_owned).or_else(||w.folder_current.clone());
            w.new_folder(crate::folders::CreateTarget::Library(parent),Some(name.to_owned()),window,cx);
        },
        "browse-import" if !w.is_shared() || w.mobile_personal_browser(cx) =>
            w.personal_action(window,cx,|w,window,cx|w.open_import(window,cx)),
        "browse-compose"=>if w.mobile_personal_browser(cx) {
            w.personal_action(window,cx,|w,window,cx|w.create_doc(false,w.folder_current.clone(),window,cx));
        } else { w.create_doc(false,w.folder_current.clone(),window,cx); },
        _=>{}
    }
}
