//! The iPad browser remains independent of the live document view.
use crate::{app::{Workspace,View}, settings::{self,tr,display_title}};
use gpui::*;
use std::{cell::RefCell,collections::HashMap};

thread_local! { static QUERIES: RefCell<HashMap<String,String>> = RefCell::new(HashMap::new()); }

pub(crate) fn scope(w: &Workspace) -> String { format!("{:?}|{:?}|{:?}",w.store.root(),w.folder_current,w.home.tab) }

fn entries(w: &Workspace,cx: &App) -> Vec<crate::store::Entry> { w.mobile_visible_entries(cx) }

pub(crate) fn project(w: &Workspace,cx: &App) -> Option<serde_json::Value> {
    if unsafe { crate::ipad::fei_ios_is_phone() } { return None; }
    let title = w.folder_current.as_ref().and_then(|id|w.entries.iter().find(|e|&e.id==id)).map(|e|display_title(&e.title))
        .unwrap_or_else(||if w.home.tab==crate::home::HomeTab::Shared { tr("共享文档","Shared documents").into() } else if w.home.tab==crate::home::HomeTab::Starred { tr("收藏","Favorites").into() } else { tr("主页","Home").into() });
    let query = w.query(cx);
    let rows = entries(w,cx).into_iter().map(|entry|serde_json::json!({"id":entry.id,"title":display_title(&entry.title),"folder":entry.meta.folder.is_some(),"pinned":entry.meta.pinned,
        "summary":crate::mobile::list_summary(&entry.body,&query)})).collect::<Vec<_>>();
    Some(serde_json::json!({"folder":w.folder_current,"favorites":w.home.tab==crate::home::HomeTab::Starred,"shared":!w.is_shared() && w.home.tab==crate::home::HomeTab::Shared,"title":title,
        "listing":{"kind":if !w.is_shared() && w.home.tab==crate::home::HomeTab::Shared { "shared" } else { "notes" },"can_reorder":w.mobile_can_reorder(cx),"selection":w.mobile_batch_state(cx),"shared_count":w.shared_entries(cx).len(),"notice_kind":w.toast.as_ref().map(|(kind,_,_)|match kind { crate::ui::ToastKind::Success=>"success",crate::ui::ToastKind::Error=>"error",crate::ui::ToastKind::Info=>"info" }),"interface_size":settings::prefs().ui_font,"notice":w.toast.as_ref().filter(|(_,_,time)|time.elapsed().as_secs_f32()<3.).map(|(_,text,_)|text.to_string()),"scope":scope(w),"query":w.search.read(cx).text,"tab":0,"tabs":false,
        "empty":if query.is_empty(){tr("选择文件夹或新建文档","Choose a folder or create a document")}else{tr("没有找到匹配的结果","No matching documents")},"rows":rows}}))
}

fn select_scope(w: &mut Workspace,target: String,window: &mut Window,cx: &mut Context<Workspace>) {
    let folder = target.strip_prefix("folder:").map(str::to_owned);
    if let Some(id) = &folder { if !w.entries.iter().chain(w.personal_navigation_entries(cx).iter()).any(|e|&e.id==id&&e.meta.folder.is_some()) { return; } }
    else if !matches!(target.as_str(),"all"|"favorites"|"shared") { return; }
    let tab = if target=="shared" { crate::home::HomeTab::Shared } else if target=="favorites" { crate::home::HomeTab::Starred } else { crate::home::HomeTab::Recent };
    if w.is_shared() && folder.as_ref().is_none_or(|id|!w.entries.iter().any(|e|&e.id==id)) { w.personal_action(window,cx,move|w,window,cx|select_scope(w,target,window,cx)); return; }
    if w.folder_current==folder && w.home.tab==tab && w.view==View::Doc { cx.notify();return; }
    let retry = target.clone();
    if !w.save_before(cx,move|w,window,cx|select_scope(w,retry,window,cx)) { return; }
    QUERIES.with(|queries|queries.borrow_mut().insert(scope(w),w.search.read(cx).text.clone()));
    w.mobile_batch=None; w.batch=None; w.mobile_navigation.links.clear(); w.mobile_navigation.trash=None;
    w.folder_current=folder;
    w.home.tab=tab;
    let query=QUERIES.with(|queries|queries.borrow().get(&scope(w)).cloned().unwrap_or_default());
    w.search.update(cx,|field,cx|field.set_text(&query,cx));
    w.show_view(if w.folder_current.is_some(){View::Folder}else{View::Home},cx);
    let _=window;
}

pub(crate) fn command(w:&mut Workspace,action:&str,value:Option<&str>,id:Option<&str>,expected:Option<&str>,window:&mut Window,cx:&mut Context<Workspace>) {
    if unsafe { crate::ipad::fei_ios_is_phone() } || w.library_switching || w.store.file_task_busy() || w.file_history.busy
        || w.confirm.is_some() || expected!=Some(scope(w).as_str()) { return; }
    match action {
        "browse-scope"=>if let Some(value)=value { select_scope(w,value.into(),window,cx); },
        "browse-search"=>if let Some(value)=value { w.search.update(cx,|field,cx|field.set_text(value,cx));cx.notify(); },
        "browse-open"=>if let Some(id)=id { if !w.is_shared() && w.mobile_collection(id,cx) { return; } if let Some(entry)=entries(w,cx).iter().find(|e|e.id==id) { if entry.meta.folder.is_some() && w.shared_list_room(id,cx).is_none() { select_scope(w,format!("folder:{id}"),window,cx); } else { w.mobile_open_entry(id,window,cx); } } },
        "browse-new-folder"=>if let Some(name)=value.filter(|v|!v.trim().is_empty()) {
            let parent=id.map(str::to_owned).or_else(||w.folder_current.clone());
            if parent.as_ref().is_some_and(|id|!w.entries.iter().any(|e|&e.id==id && e.meta.folder.is_some())) { return; }
            w.new_folder(parent,window,cx);
            if let Some(rename)=&w.rename { rename.field.update(cx,|field,cx|field.set_text(name,cx)); }
            w.finish_rename(true,window,cx);
        },
        "browse-compose"=>w.create_doc(false,w.folder_current.clone(),window,cx),
        _=>{}
    }
}
