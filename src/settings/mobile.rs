//! Native settings presentation data; persistence and operations stay in the shared controllers.
use super::*;
use serde_json::{Value, json};

#[derive(Serialize)]
struct Choice { value:String, title:String }
#[derive(Serialize)]
struct Row { id:String, title:String, detail:String, kind:&'static str, value:String, choices:Vec<Choice>, enabled:bool, danger:bool, preview:Option<Value> }
impl Row {
    fn new(id:impl Into<String>,title:impl Into<String>) -> Self { Self { id:id.into(),title:title.into(),detail:String::new(),kind:"action",value:String::new(),choices:vec![],enabled:true,danger:false,preview:None } }
    fn detail(mut self,text:impl Into<String>) -> Self { self.detail = text.into(); self }
    fn kind(mut self,kind:&'static str) -> Self { self.kind = kind; self }
    fn value(mut self,value:impl Into<String>) -> Self { self.value = value.into(); self }
    fn choices(mut self,items:impl IntoIterator<Item=(String,String)>) -> Self { self.kind = "choice"; self.choices = items.into_iter().map(|(value,title)|Choice { value,title }).collect(); self }
    fn enabled(mut self,on:bool) -> Self { self.enabled = on; self }
    fn danger(mut self) -> Self { self.danger = true; self }
}
fn choices(id:&str,title:&str,value:&str,items:&[(&str,&str)]) -> Row {
    Row::new(id,title).value(value).choices(items.iter().map(|(value,title)|(value.to_string(),title.to_string())))
}
fn section(title:&str,rows:Vec<Row>) -> Value { json!({"title":title,"rows":rows}) }
fn pages() -> Vec<Page> { vec![Page::General,Page::Appearance,Page::Library,Page::Shortcuts,Page::About] }

impl Workspace {
    fn native_settings_scope(&self) -> String {
        format!("{:?}|{}|{}|{:?}|{:?}|{}",self.store.root(),self.mobile_settings_root,self.mobile_settings_shared_library,self.settings_page,self.sheet_room().map(|room|&room.scope),if self.settings_page==Page::Appearance { prefs().theme_id() } else { String::new() })
    }
    pub(crate) fn native_settings(&self,cx:&App) -> Option<Value> {
        if self.view != View::Settings { return None; }
        let p = prefs();
        let mut sections = vec![];
        if self.mobile_settings_root {
            sections.push(section("",pages().into_iter().map(|page|Row::new(format!("page:{page:?}"),page.title()).kind("page")).collect()));
        } else { match self.settings_page {
            Page::General => {
                sections.push(section("",vec![
                    Row::new("language",tr("界面语言","Interface language")).value(p.lang().code()).choices(Lang::ALL.into_iter().map(|l|(l.code().into(),l.name().into()))),
                    Row::new("remote-images",tr("自动下载网络图片","Download web images")).kind("toggle").value(p.remote_images.to_string()).detail(tr("关闭后，网络图片显示为占位，可点击单独加载。","When off, web images show a placeholder you can tap to load.")),
                    choices("search-engine",tr("网页搜索","Web search"),&p.search_engine,&[("","Google"),("baidu",tr("百度","Baidu")),("bing","Bing"),("duckduckgo","DuckDuckGo")]),
                    Row::new("reminders",tr("日期提醒通知","Reminder notifications")).kind("page").detail(tr("查看通知权限和已安排的提醒。","View notification permission and scheduled reminders.")),
                ]));
            }
            Page::Appearance => {
                sections.push(section("",vec![
                    choices("appearance",tr("外观","Appearance"),&p.appearance,&[("",tr("跟随系统","System")),("light",tr("浅色","Light")),("dark",tr("深色","Dark"))]),
                    choices("motion",tr("动画","Animation"),&p.motion,&[("",tr("克制","Low")),("high",tr("丰富","High"))]).detail(tr("控制正文展开、选中等效果。页面切换遵循系统的减少动态效果设置。","Controls folding and selection in the editor. Navigation follows the system Reduce Motion setting.")),
                    choices("ui-font",tr("界面字号","Interface size"),&p.ui_font,&[("small",tr("小","Small")),("",tr("标准","Default")),("large",tr("大","Large"))]),
                ]));
                let list = themes::themes();
                let current = p.theme_id();
                let mut rows = vec![Row::new("theme",tr("文档主题","Document theme")).value(&current).choices(list.iter().map(|t|(t.id.clone(),t.name().into())))];
                if let Some(t) = list.iter().find(|t|t.id == current) {
                    let st = t.style;
                    let rgba = |c:Hsla| {let c = c.to_rgb(); [c.r,c.g,c.b,c.a]};
                    let text = st.color("page.color").unwrap_or_else(||st.token("text"));
                    let mut preview = Row::new("theme-preview",tr("预览","Preview")).kind("preview");
                    preview.preview = Some(json!({"font":st.font("page.font"),"mono":st.font("pre.font"),"size":crate::editor_theme::body_size(),"heading_size":crate::editor_theme::heading_metrics(1).size,"line":crate::editor_theme::body_line(),"text":rgba(text),"heading":rgba(st.color("h1.color").unwrap_or(text)),"background":rgba(st.color("page.bg").unwrap_or_else(||st.token("bg_body"))),"link":rgba(st.color("a.color").unwrap_or(text)),"code_background":rgba(st.color("pre.bg").unwrap_or_else(||st.token("code_bg"))),"title":tr("写作基础","Writing basics"),"body":tr("在飞记中记录想法，整理文档。","Capture ideas and organize documents in Fei Notes."),"list":tr("清晰的标题与列表","Clear headings and lists"),"quote":tr("让想法自然展开。","Give your ideas room to grow.")}));
                    rows.push(preview);
                    if THEME_CSS {
                        rows.push(Row::new("theme-edit",tr("编辑 CSS","Edit CSS")).kind("page"));
                        rows.push(Row::new("theme-duplicate",tr("复制为新主题","Duplicate theme")));
                        if t.customized() || t.bundled.is_none() { rows.push(Row::new("theme-remove",if t.customized() { tr("恢复默认","Restore default") } else { tr("删除主题","Delete theme") }).danger()); }
                    }
                    for (i,w) in t.warnings.iter().enumerate() { rows.push(Row::new(format!("warning:{i}"),tr("未生效","Ignored")).detail(w).kind("info")); }
                }
                drop(list);
                for (i,e) in themes::errors().into_iter().enumerate() { rows.push(Row::new(format!("theme-error:{i}"),tr("主题错误","Theme error")).detail(e).kind("info")); }
                sections.push(section(tr("文档主题","Document theme"),rows));
            }
            Page::Library => {
                let personal = !self.is_shared() || self.settings_show_personal_library(cx);
                let mut rows = vec![Row::new("library-info",tr("当前文档库","Current library")).kind("info").value(if !personal { tr("共享文档","Shared document") } else if crate::cloud_restore::local_only() { tr("本机","On this device") } else { "iCloud" })];
                if !personal {
                    rows.push(Row::new("return-personal",tr("返回我的文档库","Return to my library")));
                    sections.push(section(tr("文档库","Library"),rows));
                    sections.push(section(tr("iCloud 同步","iCloud Sync"),self.native_sync_rows(self.store.root(),cx)));
                } else {
                    rows.push(Row::new("open-trash",tr("回收站","Trash")).kind("page").detail(crate::localized_format!("删除的文档保留 {} 天后自动清除。","Deleted documents are kept for {} days.",crate::store::TRASH_DAYS)));
                    let history_limit = if self.is_shared() { cx.global::<crate::shared_store::SharedStore>().settings().history_limit() } else { self.history_limit };
                    rows.push(Row::new("history-limit",tr("版本历史","Version history")).value(history_limit.to_string()).detail(tr("每篇文档保留的历史版本数（1–30 份）","Snapshots kept for each document (1–30)")).choices((1..=30).map(|n|(n.to_string(),n.to_string()))));
                    sections.push(section(tr("文档库","Library"),rows));
                    sections.push(section(tr("导入","Import"),vec![Row::new("import-legacy",tr("从 Quiet.md 导入","Import from Quiet.md")).detail(tr("将文档复制到当前文档库。","Copy documents into your current library."))]));
                    self.native_cloud_sections(cx,&mut sections);
                    rows = vec![
                        Row::new("backup",tr("导出文档库备份","Export library backup")).detail(tr("包含文档、文件夹、回收站、附件和待同步改动。","Includes documents, folders, Trash, attachments, and pending changes.")),
                        Row::new("restore-backup",tr("从备份恢复","Restore from backup")).detail(tr("飞记会校验备份，并保留恢复前的文档库。","Fei Notes verifies the backup and keeps your current library.")),
                    ];
                    sections.push(section(if self.is_shared() { tr("文档库","Library") } else { tr("文档库备份","Library backup") },rows));
                }
            }
            Page::Shortcuts => {
                let mut rows = crate::shortcuts::rows(cx).into_iter().map(|(id,title,key)|Row::new(format!("shortcut:{id}"),title).kind("shortcut").value(key)).collect::<Vec<_>>();
                rows.push(Row::new("shortcuts-reset",tr("全部恢复默认","Reset all shortcuts")).danger());
                sections.push(section(tr("外接键盘","Hardware keyboard"),rows));
                if let Some((_,error)) = &self.shortcut_error { sections.push(section("",vec![Row::new("shortcut-error",error).kind("info")])); }
            }
            Page::Cloud => self.native_cloud_sections(cx,&mut sections),
            Page::Shared => self.native_shared_sections(cx,&mut sections),
            Page::About => {
                let version = release_bundle_string(c"CFBundleShortVersionString").unwrap_or_default();
                let build = release_bundle_string(c"CFBundleVersion").unwrap_or_default();
                sections.push(section("",vec![Row::new("identity",crate::app_name()).kind("brand").value(format!("{version} ({build})")).detail(super::product_about_description())]));
                let mut rows = vec![];
                for (key,id,title) in [(c"FeiSupportURL","support",tr("支持","Support")),(c"FeiPrivacyURL","privacy",tr("隐私政策","Privacy Policy")),(c"FeiTermsURL","terms",tr("使用条款","Terms of Use"))] {
                    if release_url(key).is_some() { rows.push(Row::new(id,title).kind("link")); }
                }
                rows.push(Row::new("notices",tr("开源许可与第三方声明","Open Source Licenses and Notices")).kind("page"));
                sections.push(section("",rows));
                sections.push(section("",vec![Row::new("attribution",tr("基于 Quiet.md 开发","Based on Quiet.md")).kind("info")]));
            }
        } }
        if self.mobile_document_sharing() {
            for section in &mut sections {
                if let Some(rows) = section["rows"].as_array_mut() {
                    rows.retain(|row| row["id"] != "share-cancel");
                }
            }
        }
        let message = self.toast.as_ref().filter(|(_,_,time)|time.elapsed().as_secs_f32()<3.).map(|(_,text,_)|text.to_string());
        Some(json!({"scope":self.native_settings_scope(),"page":if self.mobile_settings_root { "root".into() } else { format!("{:?}",self.settings_page) },"sections":sections,"message":message,"interface_size":p.ui_font}))
    }

    fn native_cloud_sections(&self,cx:&App,sections:&mut Vec<Value>) {
        let mut groups=vec![];
        self.native_cloud_sections_inner(cx,&mut groups);
        let rows=groups.into_iter().flat_map(|mut group|group["rows"].take().as_array().cloned().unwrap_or_default()).collect::<Vec<_>>();
        sections.push(json!({"title":tr("iCloud 同步","iCloud Sync"),"rows":rows}));
    }

    fn settings_show_personal_library(&self, cx: &App) -> bool {
        !self.mobile_settings_shared_library && self.mobile_personal_browser(cx)
    }

    fn native_sync_rows(&self, root: &std::path::Path, cx: &App) -> Vec<Row> {
        use crate::cloud_sync::SyncPhase;
        let mut rows = vec![Row::new("sync-state",tr("同步状态","Sync status")).kind("info")
            .detail(super::cloud_sync_detail(self,root,cx).unwrap_or_else(||tr("正在连接 iCloud","Connecting to iCloud").into()))];
        if let Some(status) = crate::cloud_sync::status(root,cx) {
            if matches!(status.phase,SyncPhase::AccountChanged|SyncPhase::SignedOut) {
                rows.push(Row::new("reconnect",tr("重新连接 iCloud…","Reconnect to iCloud…")));
            } else if !status.access_paused && !matches!(status.phase,SyncPhase::AccountChecking|SyncPhase::Revoked) {
                rows.push(Row::new("sync-retry",tr("立即同步","Sync now")));
            }
        }
        rows
    }

    fn native_cloud_sections_inner(&self,cx:&App,sections:&mut Vec<Value>) {
        let local = crate::cloud_restore::local_only();
        let personal_root = crate::store::Store::root_dir();
        let root = if self.settings_show_personal_library(cx) { personal_root.as_path() } else { self.store.root() };
        let status = crate::cloud_sync::status(root,cx);
        let mut rows = if local { vec![] } else { self.native_sync_rows(root,cx) };
        rows.push(Row::new("library-switch",if local { tr("开启 iCloud 同步…","Enable iCloud Sync…") } else { tr("切换到本机…","Switch to Local…") }).detail(crate::cloud_restore::startup_fallback().unwrap_or(if local { tr("文档保存在本机。","Documents are stored on this device.") } else { tr("正在使用 iCloud 文档库。","Using your iCloud library.") })));
        sections.push(section(tr("iCloud 同步","iCloud Sync"),rows));
        if status.is_some_and(|s|s.held>0 || s.attachments>0 || s.conflicts>0) || self.sync_issues.is_some() {
            let mut rows = vec![Row::new("sync-issues",tr("未同步的内容","Items That Couldn’t Sync")).value(if self.sync_issues.is_some() { tr("收起","Hide details") } else { tr("查看详情","Show details") })];
            if let Some(issues) = &self.sync_issues {
                for issue in issues {
                    let detail = super::sync_issue_detail(issue);
                    if let Some(title) = issue.title.as_deref() {
                        rows.push(Row::new(format!("sync-issue:{}",issue.id),display_title(title)).detail(detail).kind("page"));
                    } else {
                        rows.push(Row::new("sync-conflict",tr("尚未载入的文档", "Document not yet loaded")).detail(detail).kind("info"));
                    }
                }
                if issues.is_empty() { rows.push(Row::new("no-issues",tr("没有待处理的项目","No items need attention")).kind("info")); }
            }
            sections.push(section("",rows));
        }
        if crate::cloud_trial::testflight() { sections.push(section(tr("云功能","Cloud features"),vec![Row::new("beta","TestFlight").kind("info").detail(tr("内测期间可直接使用 iCloud 同步和协作。","iCloud sync and collaboration are available during beta testing."))])); return; }
        let s = &self.subscription;
        let mut rows = vec![];
        if cloud_subscription::available() {
            rows.push(Row::new("plans",tr("查看方案","View plans")).enabled(!s.busy).detail(s.message.clone().unwrap_or_else(||tr("本机文档库免费使用。订阅包含 iCloud 同步和协作。","The local library is free to use. A subscription includes iCloud sync and collaboration.").into())));
            for offer in &s.offers {
                let (id,title) = if offer.plan == Plan::Annual { ("purchase-annual",tr("年订阅","Annual")) } else { ("purchase-monthly",tr("月订阅","Monthly")) };
                let current = s.active.and_then(ActiveSubscription::plan_now)==Some(offer.plan);
                rows.push(Row::new(id,title).value(if current { tr("当前订阅","Current plan").into() } else { offer.price.clone() }).enabled(!s.busy && !current));
            }
            rows.push(Row::new("purchase-restore",tr("恢复购买","Restore purchases")).enabled(!s.busy));
            rows.push(Row::new("purchase-manage",tr("管理订阅","Manage subscriptions")));
        }
        if crate::cloud_trial::available() {
            let detail = s.trial_message.clone().unwrap_or_else(||s.trial.map(|t|{let (y,m,d,_,_) = crate::time::civil(t.expires_at/1000); format!("{} {y}-{m:02}-{d:02}",tr("试用截止于","Trial ends"))}).unwrap_or_else(||tr("同一 iCloud 账户的设备共用 7 天。","Devices on the same iCloud account share one seven-day trial.").into()));
            rows.push(Row::new("trial",tr("iCloud 试用","iCloud trial")).detail(detail).enabled(!s.trial_busy && s.trial.is_none()));
        }
        if !rows.is_empty() { sections.push(section(tr("云功能","Cloud features"),rows)); }
    }

    fn native_shared_sections(&self,cx:&App,sections:&mut Vec<Value>) {
        if crate::cloud_restore::local_only() { sections.push(section("",vec![Row::new("cloud-required",tr("请先打开 iCloud 文档库。","Open your iCloud library first.")).kind("info")])); return; }
        let s = &self.sharing;
        use crate::cloud_sync::shared::Audience;
        use crate::sharing::{Sheet, close_link_warning, link_audience, private_link_requirements, public_link_requirements};
        match s.sheet.as_ref() {
            Some(Sheet::RenameCollection(_)) => sections.push(section(tr("重命名个人文件夹","Rename personal folder"),vec![
                Row::new("share-folder-name",tr("名称","Name")).kind("input").value(s.folder_name.read(cx).text.clone()).detail(tr("只有你能看到这个分类。","Only you can see this folder.")).enabled(!s.busy),
                Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy)])),
            Some(Sheet::MoveInto(ids,_,room)) => sections.push(section(tr("移入共享文件夹","Move into shared folder"),vec![
                Row::new("share-effect",crate::localized_format!("将 {} 项移入「{}」。所有成员都能访问移入的文档及子文件夹。","Move {} items into “{}”. All members can access the moved documents and subfolders.",ids.len(),room.title)).kind("info"),
                Row::new("share-submit",tr("移入","Move")).enabled(!s.busy),Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy)])),
            Some(Sheet::Create(id)) => {
                let folder = self.entries.iter().any(|e| &e.id == id && e.meta.folder.is_some());
                let enabled = !s.busy && self.sharing_allowed(cx);
                let mut rows = vec![Row::new("share-effect", if folder {
                    tr("成员可访问这个文件夹、子文件夹及今后放入的文档。", "Members can access this folder, its subfolders, and items added later.")
                } else {
                    tr("共享这篇原文。你可以继续从原文件夹打开它。", "Share this document in place. You can still open it from its original folder.")
                }).kind("info")];
                if self.can_choose_link(id) {
                    rows.push(choices("share-audience",tr("访问范围","Who can access"),if s.public_link {"link"} else {"invited"},
                        &[("invited",link_audience(Audience::Invited)),("link",tr("任何持有链接的人","Anyone with the link"))]).enabled(!s.busy));
                }
                if s.public_link {
                    rows.push(Row::new("share-link-details",link_audience(Audience::LinkReadOnly)).detail(public_link_requirements()).kind("info"));
                    rows.push(Row::new("share-submit",tr("创建并复制链接","Create and copy link")).enabled(enabled));
                } else {
                    rows.push(Row::new("share-invite-reader",tr("邀请只读","Invite to view")).kind("input").detail(tr("对方的 Apple 账户邮箱","Their Apple Account email")).enabled(enabled));
                    rows.push(Row::new("share-invite-writer",tr("邀请编辑","Invite to edit")).kind("input").detail(tr("对方的 Apple 账户邮箱","Their Apple Account email")).enabled(enabled));
                }
                rows.push(Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy));
                sections.push(section(if folder {tr("共享文件夹","Share folder")} else {tr("共享文档","Share document")},rows));
            }
            Some(Sheet::End(room)) => sections.push(section(&room.title,vec![
                Row::new("share-effect",if self.stopping_original(room,cx) {tr("成员将失去访问权限，最新内容保留在原文件夹中。","Members lose access. The latest content stays in your original folder.")} else if room.scope.owner.is_none() {tr("删除后，所有参与者都将无法访问。","After deletion, all participants lose access.")} else {tr("离开后，其他参与者仍可访问。","After you leave, other participants keep access.")}).kind("info"),
                Row::new("share-submit",if self.stopping_original(room,cx) {tr("停止共享","Stop sharing")} else if room.scope.owner.is_none() {tr("删除共享文档","Delete shared document")} else {tr("离开共享","Leave share")}).danger().enabled(!s.busy),
                Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy)])),
            Some(Sheet::EndMany(rooms,personal)) => {
                let owned = rooms.iter().filter(|room|room.scope.owner.is_none()).count();
                sections.push(section(tr("移除所选文档","Remove selected documents"),vec![
                    Row::new("share-effect",crate::localized_format!("个人文档入回收站：{}；删除你创建的共享：{}；离开他人共享：{}。","Personal documents to Trash: {}; delete owned shares: {}; leave received shares: {}.",personal.len(),owned,rooms.len()-owned)).kind("info"),
                    Row::new("share-submit",tr("确认移除","Confirm removal")).danger().enabled(!s.busy),
                    Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy)]));
            }
            _ => {}
        }
        if matches!(s.sheet,Some(Sheet::Create(_) | Sheet::End(_) | Sheet::EndMany(..) | Sheet::RenameCollection(_) | Sheet::MoveInto(..))) {
            if let Some((_,message)) = &s.message { sections.push(section("",vec![Row::new("share-message",message).kind("info")])); }
            if s.busy { sections.push(section("",vec![Row::new("share-busy",tr("正在连接 iCloud","Contacting iCloud")).kind("info")])); }
            return;
        }
        if self.is_shared() {
            let role = match shared::access(&self.store,cx) { Some(Access::Owner)=>tr("所有者","Owner"),Some(Access::ReadWrite)=>tr("可编辑","Can edit"),Some(Access::ReadOnly)=>tr("只读","Can view"),Some(Access::Revoked)=>tr("访问已撤销","Access revoked"),None=>tr("正在确认权限…","Checking access…") };
            sections.push(section("",vec![Row::new("access",tr("你的权限","Your access")).value(role).kind("info")]));
            if self.sharing_access(cx)!=Some(Access::Owner) { return; }
        } else if self.sheet_room().is_none() {
            let mut rows = vec![Row::new("share-accept",tr("接受邀请","Accept invitation")).kind("input").detail(tr("粘贴飞记或 iCloud 邀请链接","Paste a Fei Notes or iCloud invitation link")).value(s.link.read(cx).text.clone()).enabled(!s.busy)];
            for room in shared::rooms(&crate::store::Store::root_dir()).unwrap_or_default() {
                rows.push(Row::new(format!("room:{}",room.scope.note),if room.title.is_empty() { tr("共享文档","Shared document").into() } else { room.title.clone() }).kind("page").detail(if room.scope.owner.is_none() { tr("由你共享","Shared by you") } else { tr("与你共享","Shared with you") }).enabled(!s.busy));
                if room.scope.owner.is_none() { rows.push(Row::new(format!("manage:{}",room.scope.note),format!("{} · {}",tr("管理","Manage"),room.title)).enabled(!s.busy)); }
            }
            sections.push(section("",rows));
        }
        if let Some(room) = self.sheet_room().filter(|room|room.scope.owner.is_none()) {
            let mut rows = vec![Row::new("share-refresh",tr("刷新成员","Refresh members")).enabled(!s.busy)];
            if let Some(reply) = &s.reply {
                let invited = reply.audience.can_invite();
                rows.push(Row::new("share-audience-info",link_audience(reply.audience)).kind("info"));
                if reply.audience == Audience::LinkDraft {
                    rows.push(Row::new("share-draft-details",tr("仍可从共享列表打开这篇文档，也可以邀请指定账户。", "You can still open this document from the shared list or invite specific accounts.")).kind("info"));
                    if self.can_retry_public_link(room,cx) {
                        rows.push(Row::new("share-retry-link",tr("重试启用并复制链接","Retry and copy link")));
                    }
                }
                if invited {
                    if reply.audience == Audience::Invited { rows.push(Row::new("share-invited-details",private_link_requirements()).kind("info")); }
                    for (id,label) in [("invite-reader",tr("邀请只读","Invite to view")),("invite-writer",tr("邀请编辑","Invite to edit"))] {
                        rows.push(Row::new(id,label).kind("input").detail(tr("对方的 Apple 账户邮箱","Their Apple Account email")).enabled(!s.busy && self.sharing_allowed(cx)));
                    }
                }
                if reply.url.is_some() {
                    rows.push(Row::new("share-copy",if invited {tr("复制邀请链接","Copy invitation link")} else {tr("复制公开链接","Copy public link")}).enabled(!s.busy));
                }
                if reply.audience.link_active() {
                    rows.push(Row::new("share-link-details",if s.close_link_confirm {close_link_warning()} else {public_link_requirements()}).kind("info"));
                    rows.push(Row::new("share-close-link",if s.close_link_confirm {tr("确认关闭链接","Confirm close link")} else {tr("关闭公开链接…","Close public link…")})
                        .danger().enabled(!s.busy && self.can_close_link(room,cx)));
                    if s.close_link_confirm { rows.push(Row::new("share-cancel-close-link",tr("取消","Cancel")).enabled(!s.busy)); }
                }
                for (index,member) in reply.members.iter().enumerate() {
                    let id = format!("member:{}",member.id);
                    let name = crate::sharing::member_label(&member.name,&member.id,index);
                    let status = if member.accepted { tr("已加入","Joined") } else { tr("待接受","Invitation pending") };
                    if invited {
                        rows.push(choices(&id,&name,if member.permission==Access::ReadWrite { "write" } else { "read" },&[("read",tr("只读","Can view")),("write",tr("可编辑","Can edit")),("revoke",tr("撤销访问","Revoke access"))]).detail(status).enabled(!s.busy && self.sharing_allowed(cx)));
                    } else {
                        rows.push(Row::new(id,name).value(if member.permission==Access::ReadWrite {tr("可编辑","Can edit")} else {tr("只读","Can view")}).detail(status).kind("info"));
                    }
                }
            }
            sections.push(section(&room.title,rows));
        }
        if let Some((_,message)) = &s.message { sections.push(section("",vec![Row::new("share-message",message).kind("info")])); }
        if s.busy { sections.push(section("",vec![Row::new("share-busy",tr("正在连接 iCloud","Contacting iCloud")).kind("info")])); }
    }

    pub(crate) fn native_settings_command(&mut self,id:&str,value:&str,scope:&str,window:&mut Window,cx:&mut Context<Self>) {
        if self.library_switching || self.view!=View::Settings || scope!=self.native_settings_scope() { return; }
        let Some(state) = self.native_settings(cx) else { return; };
        let row = state["sections"].as_array().into_iter().flatten().flat_map(|s|s["rows"].as_array().into_iter().flatten()).find(|r|r["id"].as_str()==Some(id));
        let Some(row) = row.filter(|r|r["enabled"]==true && r["kind"]!="info" && r["kind"]!="brand" && r["kind"]!="preview") else { return; };
        if row["kind"]=="choice" && !row["choices"].as_array().is_some_and(|a|a.iter().any(|c|c["value"].as_str()==Some(value))) { return; }
        if let Some(name) = id.strip_prefix("page:") {
            if let Some(page) = pages().into_iter().find(|p|format!("{p:?}")==name) {
                self.mobile_settings_parent = (!self.mobile_settings_root && self.settings_page==Page::Library && page==Page::Cloud).then_some(Page::Library);
                self.mobile_settings_root=false; self.open_settings(page,cx);
                if page==Page::Appearance { themes::load(&prefs().theme_id(),true,||cx.text_system().all_font_names()); self.reload_themes(cx); }
                if page==Page::Shared { self.mobile_shared_settings(window,cx); }
                if page==Page::Cloud {
                    if cloud_subscription::available() { self.subscription_updated(window,cx); }
                    if crate::cloud_trial::available() { self.trial_action(false,window,cx); }
                }
            }
            return;
        }
        if self.settings_page==Page::Library && self.settings_show_personal_library(cx) {
            if id=="sync-retry" { crate::cloud_sync::retry(&crate::store::Store::root_dir(),cx); return; }
            if matches!(id,"open-trash"|"history-limit"|"import-legacy"|"backup"|"restore-backup"|"sync-issues") || id.starts_with("sync-issue:") {
                let (id,value) = (id.to_owned(),value.to_owned());
                self.personal_action(window,cx,move|w,window,cx| {
                    w.mobile_open_settings(cx);
                    w.mobile_settings_root=false; w.open_settings(Page::Library,cx);
                    let scope=w.native_settings_scope();
                    w.native_settings_command(&id,&value,&scope,window,cx);
                });
                return;
            }
        }
        match id {
            "language"=>self.set_pref(|p|{p.language=value.into(); p.english=value!="zh";},cx),
            "remote-images" if matches!(value,"true"|"false")=>self.set_pref(|p|p.remote_images=value=="true",cx),
            "open-trash"=>self.mobile_open_trash(cx),
            "search-engine"=>self.set_pref(|p|p.search_engine=value.into(),cx),
            "appearance"=>self.set_pref(|p|p.appearance=value.into(),cx),
            "motion"=>self.set_pref(|p|p.motion=value.into(),cx),
            "ui-font"=>self.set_pref(|p|p.ui_font=value.into(),cx),
            "theme"=>apply_theme(self,value.into(),cx),
            "theme-edit"=>self.edit_theme(&prefs().theme_id(),cx),
            "theme-duplicate"=>self.duplicate_theme(&prefs().theme_id(),cx),
            "theme-remove"=>self.remove_theme(&prefs().theme_id(),cx),
            "history-limit"=>if let Ok(n) = value.parse::<i32>() { self.change_history_limit(n-self.history_limit as i32,window,cx); },
            "reminders"=>{crate::ipad_notifications::publish(self,cx); crate::ipad_notifications::present_settings();},
            "return-personal"=>self.navigate_personal(View::Home,None,window,cx),
            "backup"=>crate::ipad::mobile_backup(self,cx),
            "restore-backup"=>self.mobile_choose_backup(window,cx),
            "import-legacy"=>self.import_legacy_library(window,cx),
            "library-switch"=>{self.confirm=Some(crate::app::Confirm::SwitchCloudLibrary {cloud:crate::cloud_restore::local_only(),copy_local:false}); cx.notify();},
            "sync-retry"=>crate::cloud_sync::retry(self.store.root(),cx),
            "reconnect"=>crate::ipad_session::request(crate::cloud_restore::LibrarySelection::Cloud,cx),
            "sync-issues"=>{if self.sync_issues.is_some() {self.sync_issues=None;} else {match crate::cloud_sync::issues(self.store.root(),cx) {Ok(issues)=>self.sync_issues=Some(issues),Err(e)=>self.show_toast_kind(ui::ToastKind::Error,&e.to_string(),cx)}} cx.notify();},
            "plans"=>{self.subscription.show_plans=true; self.subscription_action(SubscriptionAction::Load,window,cx);},
            "purchase-annual"=>self.subscription_action(SubscriptionAction::Purchase(Plan::Annual),window,cx),
            "purchase-monthly"=>self.subscription_action(SubscriptionAction::Purchase(Plan::Monthly),window,cx),
            "purchase-restore"=>self.subscription_action(SubscriptionAction::Restore,window,cx),
            "purchase-manage"=>cx.open_url("https://apps.apple.com/account/subscriptions"),
            "trial"=>{self.confirm=Some(crate::app::Confirm::StartCloudTrial); cx.notify();},
            "share-accept"=>{self.sharing.link.update(cx,|f,cx|f.set_text(value,cx)); self.accept_shared_note(window,cx);},
            "invite-reader"|"invite-writer"=>{self.sharing.email.update(cx,|f,cx|f.set_text(value,cx)); self.manage_shared_note("invite",value.trim().into(),Some(if id=="invite-reader" {Access::ReadOnly} else {Access::ReadWrite}),window,cx);},
            "share-audience"=>self.set_public_link(value=="link",window,cx),
            "share-retry-link"=>self.retry_public_link(window,cx),
            "share-copy"=>if let Some(room) = self.sheet_room().cloned() {self.copy_shared_link(room,window,cx);},
            "share-close-link"=>self.confirm_close_link(window,cx),
            "share-cancel-close-link"=>{self.sharing.close_link_confirm=false;cx.notify();},
            "share-refresh"=>self.manage_shared_note("members",String::new(),None,window,cx),
            "share-submit"=>self.submit_sharing(window,cx),
            "share-folder-name"=>if matches!(self.sharing.sheet,Some(crate::sharing::Sheet::RenameCollection(_))) {
                self.sharing.folder_name.update(cx,|f,cx|f.set_text(value,cx));self.submit_sharing(window,cx);
            },
            "share-invite-reader"|"share-invite-writer"=>{self.sharing.email.update(cx,|f,cx|f.set_text(value,cx));self.sharing.permission=if id=="share-invite-reader"{Access::ReadOnly}else{Access::ReadWrite};self.submit_sharing(window,cx);},
            "share-cancel"=>self.close_sharing(window,cx),
            "shortcuts-reset"=>self.reset_all_shortcuts(cx),
            "support"|"privacy"|"terms"=>if let Some(url) = release_url(match id {"support"=>c"FeiSupportURL","privacy"=>c"FeiPrivacyURL",_=>c"FeiTermsURL"}) {cx.open_url(&url);},
            "notices"=>if let Some(path) = std::env::current_exe().ok().and_then(|p|Some(p.parent()?.join("Open Source Licenses.json"))) {
                unsafe extern "C" {fn fei_ios_present_notices(path:*const std::ffi::c_char,title:*const std::ffi::c_char);}
                if let (Ok(p),Ok(t)) = (std::ffi::CString::new(path.to_string_lossy().as_bytes()),std::ffi::CString::new(tr("开源许可与第三方声明","Open Source Licenses and Notices"))) {unsafe {fei_ios_present_notices(p.as_ptr(),t.as_ptr());}}
            },
            _=>{
                if let Some(note) = id.strip_prefix("room:").or_else(||id.strip_prefix("manage:")) {
                    if let Some(room) = shared::rooms(&crate::store::Store::root_dir()).unwrap_or_default().into_iter().find(|r|r.scope.note==note) {
                        if id.starts_with("room:") {self.open_shared_note(&room,window,cx);} else if room.scope.owner.is_none() {self.open_member_management(room,window,cx);}
                    }
                } else if let Some(member) = id.strip_prefix("member:") {
                    self.manage_shared_note(if value=="revoke" {"revoke"} else {"permission"},member.into(),match value {"read"=>Some(Access::ReadOnly),"write"=>Some(Access::ReadWrite),_=>None},window,cx);
                } else if let Some(key) = id.strip_prefix("shortcut:") {
                    let binding = match value {"@reset"=>None,"@clear"=>Some(String::new()),_=>Some(value.into())};
                    self.shortcut_error=crate::shortcuts::change(key,binding,cx).err().map(|e|(key.into(),e.to_string())); cx.notify();
                } else if let Some(note) = id.strip_prefix("sync-issue:") {
                    if self.sync_issues.as_ref().and_then(|issues|issues.iter().find(|i|i.id==note)).is_some_and(|i|i.trashed) {self.show_view(View::Trash,cx);} else {self.open(note,window,cx);}
                }
            }
        }
    }
}
