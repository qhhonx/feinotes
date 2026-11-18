//! Native row reordering delegates to the shared tree writer and history.
use crate::{app::{View,Workspace},home::HomeTab,settings::{display_title,tr},store::Store};
use gpui::*;
use std::{cell::RefCell,collections::BTreeMap,path::PathBuf,sync::atomic::{AtomicU64,Ordering}};

pub(crate) struct ReorderSession { serial:u64, response:u64, root:PathBuf, view:View, listing:String, pub error:Option<String> }
#[derive(serde::Serialize)]
pub(crate) struct ReorderRequest { serial:u64, response:u64, title:&'static str, prompt:String, done:&'static str, saving:bool, items:Vec<Item> }
#[derive(serde::Serialize)]
struct Item { id:String, title:String, folder:bool, pinned:bool }
static SERIAL: AtomicU64 = AtomicU64::new(1);

#[derive(Clone,Default,serde::Serialize,serde::Deserialize)]
struct PrivateOrder { rule:String, ids:Vec<String> }
const ORDER_KEY: &str = "mobile-personal-list-order";
thread_local! { static ORDERS: RefCell<BTreeMap<PathBuf,BTreeMap<String,PrivateOrder>>> = RefCell::new(BTreeMap::new()); }

fn read_orders() -> anyhow::Result<BTreeMap<String,PrivateOrder>> {
    Ok(Store::sqlite_setting(ORDER_KEY)?.map(|data|serde_json::from_slice(&data)).transpose()?.unwrap_or_default())
}

impl Workspace {
    pub(crate) fn private_order_key(&self) -> Option<String> {
        match self.home.tab {
            HomeTab::Starred => Some("favorites".into()),
            HomeTab::Shared => Some(format!("shared:{}",self.sharing.collection.as_deref().unwrap_or(""))),
            _ => None,
        }
    }
    fn private_order(&self, key:&str) -> Option<PrivateOrder> {
        ORDERS.with(|cache| cache.borrow_mut().entry(Store::root_dir()).or_insert_with(||read_orders().unwrap_or_default()).get(key).cloned())
    }
    fn save_private_order(&mut self,key:String,order:PrivateOrder,cx:&mut Context<Self>) -> bool {
        let result = (|| {
            let mut orders=read_orders()?;
            orders.insert(key,order);
            Store::write_sqlite_setting(ORDER_KEY,&serde_json::to_vec(&orders)?)?;
            ORDERS.with(|cache|cache.borrow_mut().insert(Store::root_dir(),orders));
            Ok::<_,anyhow::Error>(())
        })();
        if let Err(error)=result { self.show_toast_kind(crate::ui::ToastKind::Error,&error.to_string(),cx); return false; }
        cx.notify(); true
    }
    fn apply_shared_move(&mut self, scope: String, id: &str, before: Option<&str>, rows: &[&crate::store::Entry], cx: &mut Context<Self>) -> bool {
        let saved = (|| {
            let mut orders = read_orders()?;
            orders.entry(scope).or_default().rule = "manual".into();
            let bytes = serde_json::to_vec(&orders)?;
            Ok::<_, anyhow::Error>((orders, bytes))
        })();
        let (orders, bytes) = match saved {
            Ok(saved) => saved,
            Err(error) => { self.show_toast_kind(crate::ui::ToastKind::Error, &error.to_string(), cx); return false; }
        };
        if !self.reorder_shared_listing(id, before, rows, (ORDER_KEY, &bytes), cx) { return false; }
        ORDERS.with(|cache| cache.borrow_mut().insert(Store::root_dir(), orders));
        true
    }

    pub(crate) fn mobile_order_entries(&self,mut entries:Vec<crate::store::Entry>) -> Vec<crate::store::Entry> {
        if let Some(order)=self.private_order_key().and_then(|key|self.private_order(&key)) {
            if order.rule=="manual" && self.home.tab!=HomeTab::Shared {
                let ranks: std::collections::HashMap<_,_> = order.ids.iter().enumerate().map(|(rank,id)|(id.as_str(),rank)).collect();
                entries.sort_by_key(|entry|ranks.get(entry.id.as_str()).copied().unwrap_or(usize::MAX));
            } else { entries = self.sorted_entries_with_rule(entries,&order.rule); }
        }
        entries.sort_by_key(|entry|!entry.meta.pinned);
        entries
    }
    pub(crate) fn mobile_native_sort(&mut self,key:&str,cx:&mut Context<Self>) -> bool {
        if !matches!(key,"manual"|"visited"|"modified"|"created"|"title") || !self.mobile_can_sort(cx) { return false; }
        if self.is_shared() && self.private_order_key().is_none() { return false; }
        if let Some(scope)=self.private_order_key() {
            let mut order=self.private_order(&scope).unwrap_or_default();
            if key=="manual" && order.ids.is_empty() && self.home.tab!=HomeTab::Shared {
                order.ids=self.mobile_visible_entries(cx).into_iter().map(|entry|entry.id).collect();
            }
            order.rule=key.into();
            self.save_private_order(scope,order,cx)
        } else { self.mobile_sort(key,cx) }
    }
    pub(crate) fn mobile_can_sort(&self, cx: &App) -> bool {
        if self.library_switching || self.mobile_batch.is_some() || !self.query(cx).is_empty() || self.confirm.is_some() { return false; }
        let browser = !unsafe { crate::ipad::fei_ios_is_phone() };
        let personal = !self.is_shared() || self.mobile_personal_browser(cx);
        personal &&
            (matches!(self.view,View::Home|View::Folder) || browser && self.view==View::Doc)
    }
    pub(crate) fn mobile_row_reorderable(&self,entry:&crate::store::Entry,cx:&App) -> bool {
        self.private_order_key().is_some() || !entry.meta.local() && self.shared_list_room(&entry.id,cx).is_none()
    }
    pub(crate) fn mobile_can_reorder(&self,cx:&App) -> bool {
        if !self.mobile_can_sort(cx) || self.tree_writes_pending() { return false; }
        let rows=self.mobile_visible_entries(cx);
        [false,true].into_iter().any(|pinned|rows.iter().filter(|e| (e.meta.pinned && e.meta.folder.is_none())==pinned && self.mobile_row_reorderable(e,cx)).take(2).count()==2)
    }
    pub(crate) fn mobile_move_row(&mut self,id:&str,before:Option<&str>,cx:&mut Context<Self>) -> bool {
        if !self.mobile_can_reorder(cx) { return false; }
        let rows = self.mobile_visible_entries(cx);
        let Some(entry)=rows.iter().find(|e|e.id==id && self.mobile_row_reorderable(e,cx)) else { return false; };
        let group=|row:&crate::store::Entry|row.meta.pinned && row.meta.folder.is_none();
        let before=before.filter(|v|!v.is_empty());
        if before.is_some_and(|next|next==id || !rows.iter().any(|e|e.id==next && group(e)==group(entry))) { return false; }
        if let Some(key)=self.private_order_key() {
            if self.home.tab == HomeTab::Shared {
                let siblings = rows.iter().filter(|row| group(row) == group(entry)).collect::<Vec<_>>();
                return self.apply_shared_move(key, id, before, &siblings, cx);
            }
            let mut ids: Vec<_>=rows.iter().filter(|e|e.id!=id).map(|e|e.id.clone()).collect();
            let index=before.and_then(|next|ids.iter().position(|id|id==next)).unwrap_or(ids.len());
            ids.insert(index,id.into());
            return self.save_private_order(key,PrivateOrder { rule:"manual".into(),ids },cx);
        }
        if before.is_some_and(|next|!rows.iter().any(|e|e.id==next && self.mobile_row_reorderable(e,cx))) { return false; }
        if !self.mobile_native_sort("manual",cx) { return false; }
        self.queue_tree_move(id,self.folder_current.clone(),before.map(str::to_owned),cx);
        true
    }
    pub(crate) fn open_mobile_reorder(&mut self,cx:&mut Context<Self>) {
        if !self.mobile_can_reorder(cx) { return; }
        self.mobile_reorder = Some(ReorderSession { serial:SERIAL.fetch_add(1,Ordering::Relaxed),response:0,
            root:Store::root_dir(),view:self.view,listing:crate::ipad_browser::scope(self,cx),error:None });
        cx.notify();
    }
    fn reorder_matches(&self,serial:u64,cx:&App) -> bool {
        self.mobile_can_sort(cx) && self.mobile_reorder.as_ref().is_some_and(|s|s.serial==serial && s.root==Store::root_dir()
            && s.view==self.view && s.listing==crate::ipad_browser::scope(self,cx))
    }
    pub(crate) fn mobile_reorder_request(&self,cx:&App) -> Option<ReorderRequest> {
        let session=self.mobile_reorder.as_ref()?;
        if !self.reorder_matches(session.serial,cx) { return None; }
        let saving=self.tree_writes_pending();
        let prompt=if saving { tr("正在保存…","Saving…").into() } else { session.error.clone().unwrap_or_else(||tr("拖动右侧手柄调整顺序。","Drag the handles to change the order.").into()) };
        let items=self.mobile_visible_entries(cx).into_iter().filter(|entry|self.mobile_row_reorderable(entry,cx))
            .map(|entry|Item { id:entry.id,title:display_title(&entry.title),folder:entry.meta.folder.is_some(),pinned:entry.meta.pinned }).collect();
        Some(ReorderRequest {serial:session.serial,response:session.response,title:tr("调整顺序","Reorder"),prompt,done:tr("完成","Done"),saving,items})
    }
    pub(crate) fn mobile_reorder_command(&mut self,serial:u64,action:&str,id:Option<&str>,before:Option<&str>,cx:&mut Context<Self>) {
        if action=="reorder-close" {
            if self.mobile_reorder.as_ref().is_some_and(|session|session.serial==serial) { self.mobile_reorder=None; cx.notify(); }
            return;
        }
        if action!="reorder-move" || !self.reorder_matches(serial,cx) { return; }
        let saved=id.is_some_and(|id|self.mobile_move_row(id,before,cx));
        if let Some(session)=&mut self.mobile_reorder {
            session.response+=1;
            session.error=(!saved).then(||tr("列表已更新，请重试。","The list changed. Try again.").into());
        }
        cx.notify();
    }
}
