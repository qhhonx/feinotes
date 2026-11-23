//! Native settings presentation data; persistence and operations stay in the shared controllers.
use super::*;
use serde_json::{Value, json};

#[derive(Serialize)]
struct Choice {
    value:String,
    title:String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview:Option<Value>,
}
#[derive(Serialize)]
struct Row { id:String, title:String, detail:String, kind:&'static str, value:String, choices:Vec<Choice>, enabled:bool, danger:bool, preview:Option<Value> }
impl Row {
    fn new(id:impl Into<String>,title:impl Into<String>) -> Self { Self { id:id.into(),title:title.into(),detail:String::new(),kind:"action",value:String::new(),choices:vec![],enabled:true,danger:false,preview:None } }
    fn detail(mut self,text:impl Into<String>) -> Self { self.detail = text.into(); self }
    fn kind(mut self,kind:&'static str) -> Self { self.kind = kind; self }
    fn value(mut self,value:impl Into<String>) -> Self { self.value = value.into(); self }
    fn choices(mut self,items:impl IntoIterator<Item=(String,String)>) -> Self { self.kind = "choice"; self.choices = items.into_iter().map(|(value,title)|Choice { value,title,preview:None }).collect(); self }
    fn enabled(mut self,on:bool) -> Self { self.enabled = on; self }
    fn danger(mut self) -> Self { self.danger = true; self }
}
fn choices(id:&str,title:&str,value:&str,items:&[(&str,&str)]) -> Row {
    Row::new(id,title).value(value).choices(items.iter().map(|(value,title)|(value.to_string(),title.to_string())))
}
fn section(title:&str,rows:Vec<Row>) -> Value { json!({"title":title,"rows":rows}) }
fn share_target(title:&str,folder:bool,detail:Option<&str>) -> Value {
    section(crate::sharing::sharing_target_label(folder),vec![Row::new("share-target",
        if title.trim().is_empty() { crate::sharing::sharing_target_label(folder) } else { title })
        .kind("share-target").detail(detail.unwrap_or_default())])
}
const TRASH_PAGE_ID: &str = "page:Trash";

fn theme_preview(st: &themes::Style) -> Value {
    use crate::editor_theme::{body_line_for, body_size_for, heading_metrics_for, title_for};
    let sample = theme_sample();
    let rgba = |c:Hsla| { let c = c.to_rgb(); [c.r,c.g,c.b,c.a] };
    let ink = st.color("page.color").unwrap_or_else(|| st.token("text"));
    let paper = st.color("page.bg").unwrap_or_else(|| st.token("bg_body"));
    let title = title_for(st);
    let heading = heading_metrics_for(st, 2);
    let body_size = body_size_for(st);
    let task_size = st.num("task.size");
    let task_radius = st.num("task.radius");
    let task_scale = if task_size > 0. && task_radius >= task_size / 2. - 0.1 { body_size / st.num("page.size").max(1.) } else { 1. };
    let highlight = st.named("highlight","light-yellow");
    let strong = st.color("strong.color").unwrap_or(ink);
    json!({
        "title":sample.title,
        "body":sample.body,
        "highlight":sample.highlight,
        "heading":sample.heading,
        "completed":sample.completed,
        "pending":sample.pending,
        "paper":rgba(paper),
        "title_style":{"font":st.font("title.font"),"size":title.size,"line":title.line_height,"weight":st.num("title.weight"),"ink":rgba(st.color("title.color").unwrap_or(ink))},
        "heading_style":{"font":st.font("h2.font"),"size":heading.size,"line":heading.line_height,"weight":st.num("h2.weight"),"ink":rgba(st.color("h2.color").unwrap_or(ink))},
        "heading_gap":heading.mb,
        "body_style":{"font":st.font("page.font"),"size":body_size,"line":body_line_for(st),"weight":400,"ink":rgba(ink)},
        "paragraph_gap":st.num("p.gap"),
        "highlight_fill":highlight.map(rgba),
        "highlight_ink":rgba(if highlight.is_some() { crate::theme::on_mark(strong) } else { strong }),
        "highlight_weight":st.num("strong.weight"),
        "tasks":{
            "size":task_size * task_scale,"radius":task_radius * task_scale,
            "gap":st.todo_gap.unwrap_or_else(|| st.list_gap()),
            "fill":rgba(st.color("task.checked").unwrap_or_else(|| st.token("primary"))),
            "check":rgba(st.color("task.check").unwrap_or_else(gpui::white)),
            "border":rgba(st.token("todo_box")),
            "empty":rgba(crate::theme::raised(st.token("bg_body"))),
            "done_ink":rgba(st.token("todo_done")),
            "strike":st.flag("done.strike"),
            "done_label":tr("已完成","Completed"),
            "pending_label":tr("未完成","Incomplete"),
        },
    })
}

impl Workspace {
    fn native_settings_scope(&self) -> String {
        let appearance = if self.settings_page == Page::Appearance {
            let p = prefs();
            format!("{}|{}", p.interface_style().id(), p.theme_id())
        } else { String::new() };
        let share_stage = if self.settings_page == Page::Shared { self.sharing.sheet.as_ref().map(|sheet| match sheet {
            crate::sharing::Sheet::Create(id) => ("create",Some(id.as_str())),
            crate::sharing::Sheet::Invite(_) => ("invite",None),
            crate::sharing::Sheet::Members(_) => ("members",None),
            crate::sharing::Sheet::CreateNew(..) => ("new",None),
            crate::sharing::Sheet::End(_) => ("end",None),
            _ => ("",None),
        }) } else { None };
        format!("{:?}|{}|{:?}|{:?}|{share_stage:?}|{}",self.store.root(),self.mobile_settings_root,self.settings_page,self.sheet_room().map(|room|&room.scope),appearance)
    }
    pub(crate) fn native_settings(&self,cx:&App) -> Option<Value> {
        if self.view != View::Settings { return None; }
        let p = prefs();
        let mut sections = vec![];
        if self.mobile_settings_root {
            sections.push(section("",NAV.iter().copied().map(|page| {
                let Some(page) = page else {
                    return Row::new(TRASH_PAGE_ID,tr("回收站","Trash")).kind("page").enabled(self.personal_settings_store(cx).is_some());
                };
                let row = Row::new(format!("page:{page:?}"),page.title()).kind("page");
                if page == Page::Cloud { row.value(self.cloud_summary(cx)) } else { row }
            }).collect()));
        } else { match self.settings_page {
            Page::General => {
                sections.push(section("",vec![
                    Row::new("language",tr("界面语言","Interface language")).value(p.lang().code()).choices(Lang::ALL.into_iter().map(|l|(l.code().into(),l.name().into()))),
                    Row::new("remote-images",tr("自动下载网络图片","Download web images")).kind("toggle").value(p.remote_images.to_string()).detail(tr("关闭后，网络图片显示为占位，可点击单独加载。","When off, web images show a placeholder you can tap to load.")),
                    Row::new("reminders",tr("日期提醒通知","Reminder notifications")).kind("page").detail(tr("查看通知权限和已安排的提醒。","View notification permission and scheduled reminders.")),
                ]));
            }
            Page::Appearance => {
                sections.push(section("",vec![
                    choices("appearance",tr("外观","Appearance"),&p.appearance,&[("",tr("跟随系统","System")),("light",tr("浅色","Light")),("dark",tr("深色","Dark"))]),
                    choices("ui-font",tr("界面字号","Interface size"),&p.ui_font,&[("small",tr("小","Small")),("",tr("标准","Default")),("large",tr("大","Large"))]),
                ]));
                let list = themes::themes();
                let current = p.theme_id();
                let mut picker = Row::new("theme",tr("文档主题","Document theme")).value(&current).kind("choice");
                picker.choices = list.iter().map(|t| Choice { value:t.id.clone(),title:t.name().into(),preview:Some(theme_preview(t.style)) }).collect();
                let mut preview = Row::new("theme-preview",tr("预览","Preview")).kind("preview").enabled(false);
                preview.preview = picker.choices.iter().find(|choice| choice.value == current).and_then(|choice| choice.preview.clone());
                let mut rows = vec![
                    Row::new("interface-style",tr("界面风格","Interface style")).value(p.interface_style().id())
                        .choices(InterfaceStyle::ALL.into_iter().map(|style| (style.id().into(),style.title().into()))),
                    picker,
                ];
                if let Some(t) = list.iter().find(|t|t.id == current) {
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
                sections.push(section("",rows));
            }
            Page::Library => {
                if self.personal_settings_store(cx).is_some() {
                    let rows = vec![
                        Row::new("library-info",tr("当前文档库","Current library")).kind("info").value(self.cloud_library_label(cx)),
                        Row::new("history-limit",tr("版本历史","Version history")).value(self.personal_history_limit(cx).to_string()).detail(tr("每篇文档保留的历史版本数（1–30 份）","Snapshots kept for each document (1–30)")).choices((1..=30).map(|n|(n.to_string(),n.to_string()))),
                    ];
                    sections.push(section("",rows));
                    sections.push(section(tr("导入与导出","Import and Export"),vec![
                        Row::new("import-legacy",tr("从 Quiet.md 迁移","Migrate from Quiet.md")).detail(tr("将文档复制到当前文档库。","Copy documents into your current library.")),
                        Row::new("backup",tr("导出整库文件","Export Library File")).detail(tr("包含文档、文件夹、回收站、附件和待同步改动。","Includes documents, folders, Trash, attachments and pending changes.")),
                        Row::new("restore-backup",tr("从整库文件恢复","Restore from Library File")).detail(tr("飞记会校验所选文件，并保留恢复前的文档库。","Fei Notes verifies the selected file and keeps your current library.")),
                    ]));
                } else {
                    sections.push(section("",vec![Row::new("return-personal",tr("退出共享视图","Exit shared view"))]));
                }
            }
            Page::SharedSync => {
                let root = self.store.root();
                let mut rows = self.native_sync_rows(root,cx);
                rows.extend(self.native_sync_issue_rows(root,cx));
                sections.push(section(tr("iCloud 同步","iCloud Sync"),rows));
                sections.push(section("",vec![Row::new("return-personal",tr("退出共享视图","Exit shared view"))]));
            }
            Page::Shortcuts => {
                let mut rows = crate::shortcuts::rows(cx).into_iter().map(|(id,title,key)|Row::new(format!("shortcut:{id}"),title).kind("shortcut").value(key)).collect::<Vec<_>>();
                rows.push(Row::new("shortcuts-reset",tr("全部恢复默认","Reset all shortcuts")).danger());
                sections.push(section(tr("外接键盘","Hardware keyboard"),rows));
                if let Some((_,error)) = &self.shortcut_error { sections.push(section("",vec![Row::new("shortcut-error",error).kind("info")])); }
            }
            Page::Cloud => self.native_cloud_sections(cx,&mut sections),
            Page::CloudAccess => self.native_access_sections(&mut sections),
            Page::Shared => self.native_shared_sections(cx,&mut sections),
            Page::About => {
                let version = release_bundle_string(c"CFBundleShortVersionString").unwrap_or_default();
                let build = release_bundle_string(c"CFBundleVersion").unwrap_or_default();
                sections.push(section("",vec![Row::new("identity",crate::app_name()).kind("brand").value(format!("{version} ({build})")).detail(super::product_about_description())]));
                let mut rows: Vec<Row> = about_public_links().into_iter().map(|(id,title,_)| Row::new(id,title).kind("link")).collect();
                for (key,id,title) in [(c"FeiSupportURL","support",tr("支持","Support")),(c"FeiPrivacyURL","privacy",tr("隐私政策","Privacy Policy")),(c"FeiTermsURL","terms",tr("使用条款","Terms of Use"))] {
                    if release_url(key).is_some() { rows.push(Row::new(id,title).kind("link")); }
                }
                rows.push(Row::new("notices",tr("开源许可与第三方声明","Open Source Licenses and Notices")).kind("page"));
                sections.push(json!({"title":"","rows":rows,"footer":super::upstream_attribution()}));
            }
        } }
        if self.mobile_contextual_sharing() {
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
        use super::personal_sync::{self, Action};
        if crate::cloud_restore::local_only() {
            sections.push(section("",vec![
                Row::new("cloud-off",tr("同步未开启","Sync is off")).kind("info").detail(self.enable_cloud_message()),
                Row::new("cloud-enable",tr("开启 iCloud 同步…","Enable iCloud Sync…"))
                    .enabled(!self.subscription.enable_checking),
            ]));
        } else if let Some(store) = self.personal_settings_store(cx) {
            let sync = personal_sync::read(store.root(),cx);
            let mut state = Row::new("sync-state",sync.title).kind("info");
            if let Some(detail) = sync.detail { state = state.detail(detail); }
            sections.push(section(tr("同步状态","Sync status"),vec![state]));
            sections.push(section("",sync.rows.into_iter()
                .map(|(id,label,value)|Row::new(id,label).kind("info").value(value)).collect()));
            let action = match sync.action {
                Action::Sync | Action::AutoRetry => Some(Row::new("sync-retry",tr("立即同步","Sync Now")).enabled(sync.action == Action::Sync)),
                Action::VerifyAccount => Some(Row::new("verify-cloud-account",tr("重新验证 iCloud","Verify iCloud Again"))),
                Action::SignIn | Action::ChangedAccount => Some(Row::new("account-settings",tr("登录原 iCloud 账户","Sign in to your previous iCloud account"))
                    .kind("info").detail(tr("设置 › Apple 账户","Settings › Apple Account"))),
                Action::Plans if crate::cloud_trial::available() || cloud_subscription::available() =>
                    Some(Row::new("page:CloudAccess",tr("查看订阅方案","View Plans")).kind("page")),
                _ => None,
            };
            if let Some(action) = action { sections.push(section("",vec![action])); }
            let issues = self.native_sync_issue_rows(store.root(),cx);
            if !issues.is_empty() { sections.push(section("",issues)); }
            if sync.action != Action::Plans && !crate::cloud_trial::testflight() && cloud_subscription::available() {
                sections.push(section("",vec![Row::new("page:CloudAccess",tr("查看订阅方案","View Plans")).kind("page")]));
            }
            sections.push(section("",vec![Row::new("cloud-disable",tr("关闭 iCloud 同步…","Turn Off iCloud Sync…"))
                .detail(cloud_disable_detail())]));
        }
    }

    fn native_sync_rows(&self, root: &std::path::Path, cx: &App) -> Vec<Row> {
        use crate::cloud_sync::SyncPhase;
        let mut rows = vec![Row::new("sync-state",tr("同步状态","Sync status")).kind("info")
            .detail(super::cloud_sync_detail(self,root,cx).unwrap_or_else(||tr("正在连接 iCloud","Connecting to iCloud").into()))];
        if let Some(status) = crate::cloud_sync::status(root,cx) {
            if !status.access_paused && !matches!(status.phase,SyncPhase::AccountChecking|SyncPhase::AccountChanged|SyncPhase::SignedOut|SyncPhase::Revoked) {
                rows.push(Row::new("sync-retry",tr("立即同步","Sync now")));
            }
        }
        rows
    }

    fn native_sync_issue_rows(&self, root: &std::path::Path, cx: &App) -> Vec<Row> {
        let status = crate::cloud_sync::status(root,cx);
        if !status.is_some_and(|s|s.held>0 || s.attachments>0 || s.conflicts>0) && self.sync_issues.is_none() { return vec![]; }
        let mut rows = vec![Row::new("sync-issues",tr("未同步的内容","Items That Couldn’t Sync")).value(if self.sync_issues.is_some() { tr("收起","Hide details") } else { tr("查看详情","Show details") })];
        if let Some(issues) = &self.sync_issues {
            for issue in issues {
                let detail = super::sync_issue_detail(issue);
                let row = if let Some(title) = issue.title.as_deref() {
                    Row::new(format!("sync-issue:{}",issue.id),display_title(title)).detail(detail)
                        .kind(if self.settings_page == Page::SharedSync && issue.trashed { "info" } else { "page" })
                } else {
                    Row::new("sync-conflict",tr("尚未载入的文档","Document not yet loaded")).detail(detail).kind("info")
                };
                rows.push(row);
            }
            if issues.is_empty() { rows.push(Row::new("no-issues",tr("没有待处理的项目","No items need attention")).kind("info")); }
        }
        rows
    }

    fn native_access_sections(&self,sections:&mut Vec<Value>) {
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

    fn native_invitation_section(&self,cx:&App,enabled:bool) -> Value {
        let s = &self.sharing;
        let rows = vec![
            choices("share-permission",tr("权限","Permission"),if s.permission == Access::ReadWrite { "write" } else { "read" },
                &[("read",crate::sharing::role(Some(Access::ReadOnly))),("write",crate::sharing::role(Some(Access::ReadWrite)))]).enabled(enabled),
            Row::new("share-invite",tr("创建邀请…","Create invitation…")).kind("share-email")
                .detail(tr("对方的 Apple 账户邮箱","Their Apple Account email")).value(s.email.read(cx).text.clone()).enabled(enabled),
        ];
        json!({"title":tr("指定账户","Invited accounts"),"rows":rows,"footer":crate::sharing::private_invitation_delivery()})
    }

    fn native_share_audience_section(&self) -> Value {
        section("",vec![choices("share-audience",tr("访问范围","Who can access"),if self.sharing.public_link {"link"} else {"invited"},
            &[("invited",crate::sharing::link_audience(crate::cloud_sync::shared::Audience::Invited)),("link",tr("任何持有链接的人","Anyone with the link"))])
            .enabled(!self.sharing.busy)])
    }

    fn native_public_link_section(&self, enabled: bool) -> Value {
        let editable = self.sharing.permission == Access::ReadWrite;
        let mut rows = vec![choices("share-public-permission",tr("权限","Permission"),if editable {"write"} else {"read"},
            &[("read",crate::sharing::role(Some(Access::ReadOnly))),("write",crate::sharing::role(Some(Access::ReadWrite)))])
            .enabled(!self.sharing.busy && self.can_choose_sharing_audience()),
            Row::new("share-link-details",crate::sharing::public_link_requirements()).kind("info")];
        if editable { rows.push(Row::new("public-edit-warning",crate::sharing::public_edit_warning()).kind("info")); }
        rows.push(Row::new("share-submit",tr("创建并复制链接","Create and copy link")).enabled(enabled));
        section("",rows)
    }

    fn native_shared_sections(&self,cx:&App,sections:&mut Vec<Value>) {
        let s = &self.sharing;
        let local = crate::cloud_restore::local_only();
        let cloud_required = tr("请先打开 iCloud 文档库。","Open your iCloud library first.");
        if local && !matches!(s.sheet,Some(crate::sharing::Sheet::Accept)) {
            sections.push(section("",vec![Row::new("cloud-required",cloud_required).kind("info")]));
            return;
        }
        use crate::cloud_sync::shared::Audience;
        use crate::sharing::{Sheet, close_link_warning, link_audience, private_link_requirements, public_link_requirements, sharing_folder_scope};
        match s.sheet.as_ref() {
            Some(Sheet::Accept) => {
                let mut rows = vec![Row::new("share-accept",tr("接受并打开","Accept and open")).kind("invitation-link")
                    .detail(tr("粘贴飞记或 iCloud 邀请链接","Paste a Fei Notes or iCloud invitation link"))
                    .value(s.link.read(cx).text.clone()).enabled(!s.busy && !local),
                    Row::new("share-account-requirements",crate::sharing::invitation_account_requirements()).kind("info")];
                if local { rows.push(Row::new("share-unavailable",cloud_required).kind("info")); }
                if let Some((_,message)) = &s.message { rows.push(Row::new("share-message",message).kind("info")); }
                if let Some(message) = s.busy_feedback { rows.push(Row::new("share-busy",message).kind("info")); }
                sections.push(section("",rows));
                return;
            }
            Some(Sheet::Access(room)) => {
                let (owner,access) = shared::access_summary(room,cx);
                sections.push(share_target(&room.title,room.scope.folder,self.sharing_scope_detail(room,cx)));
                sections.push(section("",vec![
                    Row::new("share-source",tr("共享来源","Shared by")).kind("info")
                        .value(owner.unwrap_or_else(||tr("与你共享","Shared with you").into()).as_ref()),
                    Row::new("access",tr("你的权限","Your access")).kind("info").value(crate::sharing::role(access)),
                ]));
                return;
            }
            Some(Sheet::RenameCollection(_)) => sections.push(section(tr("重命名个人文件夹","Rename personal folder"),vec![
                Row::new("share-folder-name",tr("名称","Name")).kind("input").value(s.folder_name.read(cx).text.clone()).detail(tr("只有你能看到这个分类。","Only you can see this folder.")).enabled(!s.busy),
                Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy)])),
            Some(Sheet::MoveInto(ids,_,room)) => sections.push(section(tr("移入共享文件夹","Move into shared folder"),vec![
                Row::new("share-effect",crate::localized_format!("将 {} 项移入「{}」。所有成员都能访问移入的文档及子文件夹。","Move {} items into “{}”. All members can access the moved documents and subfolders.",ids.len(),room.title)).kind("info"),
                Row::new("share-submit",tr("移入","Move")).enabled(!s.busy),Row::new("share-cancel",tr("取消","Cancel")).enabled(!s.busy)])),
            Some(Sheet::CreateNew(folder, id)) => {
                let enabled = !s.busy && self.sharing_allowed(cx);
                let name = s.folder_name.read(cx).text.clone();
                sections.push(section("",vec![Row::new("share-new-name", tr("名称", "Name"))
                    .kind(if id.is_some() { "info" } else { "input" }).value(name.clone()).enabled(!s.busy)]));
                if *folder {
                    sections.push(section("",vec![Row::new("share-effect",sharing_folder_scope()).kind("info")]));
                }
                if self.can_choose_sharing_audience() { sections.push(self.native_share_audience_section()); }
                if s.public_link { sections.push(self.native_public_link_section(enabled && !name.trim().is_empty())); }
                else { sections.push(self.native_invitation_section(cx,enabled && !name.trim().is_empty())); }
            }
            Some(Sheet::Create(id)) => {
                let entries = self.personal_navigation_entries(cx);
                let entry = entries.iter().find(|e| &e.id == id);
                let folder = entry.is_some_and(|e| e.meta.folder.is_some());
                let enabled = !s.busy && self.sharing_allowed(cx);
                sections.push(share_target(entry.map_or("",|e|e.title.as_str()),folder,Some(if folder {
                    sharing_folder_scope()
                } else { tr("与对方协作这篇原文。你仍可从原文件夹打开它。", "Collaborate on this document in its original folder.") })));
                if self.can_choose_sharing_audience() { sections.push(self.native_share_audience_section()); }
                if s.public_link { sections.push(self.native_public_link_section(enabled)); }
                else { sections.push(self.native_invitation_section(cx,enabled)); }
            }
            Some(Sheet::Invite(room)) => {
                sections.push(share_target(&room.title,room.scope.folder,self.sharing_scope_detail(room,cx)));
                sections.push(self.native_invitation_section(cx,!s.busy && self.sharing_allowed(cx)));
                sections.push(section("",vec![Row::new("share-back",tr("返回成员与权限","Back to members and access")).kind("action").enabled(!s.busy)]));
            }
            Some(Sheet::End(room)) => sections.push(section(&room.title,vec![
                Row::new("share-effect",self.shared_end_effect(room,cx)).kind("info"),
                Row::new("share-submit",self.shared_end_label(room,cx)).danger().enabled(!s.busy),
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
        if matches!(s.sheet,Some(Sheet::Create(_) | Sheet::CreateNew(..) | Sheet::Invite(_) | Sheet::End(_) | Sheet::EndMany(..) | Sheet::RenameCollection(_) | Sheet::MoveInto(..))) {
            if let Some((_,message)) = &s.message { sections.push(section("",vec![Row::new("share-message",message).kind("info")])); }
            if let Some(message) = s.busy_feedback { sections.push(section("",vec![Row::new("share-busy",message).kind("info")])); }
            return;
        }
        if let Some(room) = self.sheet_room().filter(|room|room.scope.owner.is_none()) {
            sections.push(share_target(&room.title,room.scope.folder,self.sharing_scope_detail(room,cx)));
            let mut people = vec![Row::new("access",tr("你","You")).value(tr("所有者","Owner")).kind("info")];
            let mut links = Vec::new();
            if let Some(reply) = &s.reply {
                let invited = reply.audience.can_invite();
                for (index,member) in reply.members.iter().enumerate() {
                    let id = format!("member:{}",member.id);
                    let name = crate::sharing::member_label(&member.name,&member.id,index);
                    let status = if member.accepted { tr("已加入","Joined") } else { tr("待接受","Invitation pending") };
                    let permission = crate::sharing::role(Some(member.permission));
                    if invited {
                        let revoke = if member.accepted { tr("移除参与者","Remove participant") } else { tr("取消邀请","Cancel invitation") };
                        people.push(choices(&id,&name,if member.permission==Access::ReadWrite { "write" } else { "read" },
                            &[("read",crate::sharing::role(Some(Access::ReadOnly))),("write",crate::sharing::role(Some(Access::ReadWrite))),("revoke",revoke)])
                            .detail(status).enabled(!s.busy && self.sharing_allowed(cx)));
                    } else {
                        people.push(Row::new(id,name).value(permission).detail(status).kind("info"));
                    }
                }
                if invited {
                    if reply.members.is_empty() {
                        people.push(Row::new("share-empty",tr("尚未邀请其他参与者","No other participants invited yet")).kind("info"));
                    }
                    people.push(Row::new("share-add",tr("添加参与者","Add participant")).kind("page").enabled(!s.busy && self.sharing_allowed(cx)));
                }
                links.push(Row::new("share-audience-info",link_audience(reply.audience)).kind("info")
                    .detail(if reply.audience == Audience::Invited { private_link_requirements() } else { "" }));
                if reply.audience == Audience::LinkDraft {
                    links.push(Row::new("share-draft-details",tr("仍可从共享列表打开这篇文档，也可以邀请指定账户。", "You can still open this document from the shared list or invite specific accounts.")).kind("info"));
                    if self.can_retry_public_link(room,cx) {
                        links.push(Row::new("share-retry-link",tr("重试启用并复制链接","Retry and copy link")));
                    }
                }
                if reply.url.is_some() {
                    links.push(Row::new("share-copy",if invited {tr("复制邀请链接","Copy invitation link")} else {tr("复制公开链接","Copy public link")}).enabled(!s.busy));
                }
                if reply.audience.link_active() {
                    links.push(Row::new("share-link-details",if s.close_link_confirm {close_link_warning()} else {public_link_requirements()}).kind("info"));
                    links.push(Row::new("share-close-link",if s.close_link_confirm {tr("确认关闭链接","Confirm close link")} else {tr("关闭公开链接…","Close public link…")})
                        .danger().enabled(!s.busy && self.can_close_link(room,cx)));
                    if s.close_link_confirm { links.push(Row::new("share-cancel-close-link",tr("取消","Cancel")).enabled(!s.busy)); }
                }
            }
            sections.push(section(tr("参与者","Participants"),people));
            if !links.is_empty() { sections.push(section(tr("链接访问","Link access"),links)); }
            sections.push(section("",vec![Row::new("share-refresh",tr("刷新成员","Refresh members")).enabled(!s.busy)]));
            sections.push(section("",vec![Row::new("share-end",self.shared_end_label(room,cx))
                .kind("destructive-page").enabled(!s.busy && self.sharing_account_ready(cx))]));
        } else if self.sheet_room().is_none() {
            let mut rows = vec![Row::new("share-accept",tr("接受邀请","Accept invitation")).kind("input").detail(tr("粘贴飞记或 iCloud 邀请链接","Paste a Fei Notes or iCloud invitation link")).value(s.link.read(cx).text.clone()).enabled(!s.busy)];
            for room in shared::rooms(&crate::store::Store::root_dir()).unwrap_or_default() {
                rows.push(Row::new(format!("room:{}",room.scope.note),if room.title.is_empty() { tr("共享文档","Shared document").into() } else { room.title.clone() }).kind("page").detail(if room.scope.owner.is_none() { tr("由你共享","Shared by you") } else { tr("与你共享","Shared with you") }).enabled(!s.busy));
                if room.scope.owner.is_none() { rows.push(Row::new(format!("manage:{}",room.scope.note),format!("{} · {}",tr("管理","Manage"),room.title)).enabled(!s.busy)); }
            }
            sections.push(section("",rows));
        }
        if let Some((_,message)) = &s.message { sections.push(section("",vec![Row::new("share-message",message).kind("info")])); }
        if let Some(message) = s.busy_feedback { sections.push(section("",vec![Row::new("share-busy",message).kind("info")])); }
    }

    pub(crate) fn native_settings_command(&mut self,id:&str,value:&str,scope:&str,window:&mut Window,cx:&mut Context<Self>) {
        if self.library_switching || self.view!=View::Settings || scope!=self.native_settings_scope() { return; }
        if self.mobile_settings_root {
            if id == TRASH_PAGE_ID {
                if self.personal_settings_store(cx).is_some() {
                    self.personal_action(window,cx,|w,_,cx|w.mobile_open_trash(cx));
                }
                return;
            }
            let Some(page) = NAV.iter().flatten().copied().find(|page|format!("page:{page:?}")==id) else { return; };
            self.mobile_settings_parent = None;
            self.mobile_settings_root = false;
            if page == Page::Cloud { self.open_cloud_settings(window,cx); }
            else {
                self.open_settings(page,cx);
                if page == Page::Appearance { themes::load(&prefs().theme_id(),true,||cx.text_system().all_font_names()); self.reload_themes(cx); }
            }
            return;
        }
        let Some(state) = self.native_settings(cx) else { return; };
        let row = state["sections"].as_array().into_iter().flatten().flat_map(|s|s["rows"].as_array().into_iter().flatten()).find(|r|r["id"].as_str()==Some(id));
        let Some(row) = row.filter(|r|r["enabled"]==true && r["kind"]!="info" && r["kind"]!="brand" && r["kind"]!="preview") else { return; };
        if row["kind"]=="choice" && !row["choices"].as_array().is_some_and(|a|a.iter().any(|c|c["value"].as_str()==Some(value))) { return; }
        match id {
            "language"=>self.set_pref(|p|{p.language=value.into(); p.english=value!="zh";},cx),
            "remote-images" if matches!(value,"true"|"false")=>self.set_pref(|p|p.remote_images=value=="true",cx),
            "appearance"=>self.set_pref(|p|p.appearance=value.into(),cx),
            "interface-style"=>self.set_pref(|p|p.interface_style=value.into(),cx),
            "ui-font"=>self.set_pref(|p|p.ui_font=value.into(),cx),
            "theme"=>apply_theme(self,value.into(),cx),
            "theme-edit"=>self.edit_theme(&prefs().theme_id(),cx),
            "theme-duplicate"=>self.duplicate_theme(&prefs().theme_id(),cx),
            "theme-remove"=>self.remove_theme(&prefs().theme_id(),cx),
            "history-limit"=>if let Ok(n) = value.parse::<i32>() { self.change_history_limit(n-self.personal_history_limit(cx) as i32,window,cx); },
            "reminders"=>{crate::ipad_notifications::publish(self,cx); crate::ipad_notifications::present_settings();},
            "return-personal"=>self.mobile_return_from_share(window,cx),
            "backup"=>crate::ipad::mobile_backup(self,cx),
            "restore-backup"=>self.mobile_choose_backup(window,cx),
            "import-legacy"=>self.personal_action(window,cx,|w,window,cx|w.import_legacy_library(window,cx)),
            "cloud-enable"=>self.begin_cloud_enable(window,cx),
            "page:CloudAccess"=>self.open_cloud_access(window,cx),
            "cloud-disable"=>self.begin_cloud_disable(cx),
            "sync-retry"=>if let Some(root) = self.settings_sync_root(cx)
                && (self.settings_page != Page::Cloud || super::personal_sync::read(&root,cx).action == super::personal_sync::Action::Sync) {
                crate::cloud_sync::retry(&root,cx);
            },
            "verify-cloud-account" if self.settings_page == Page::Cloud =>
                if let Some(root) = self.settings_sync_root(cx) { crate::cloud_sync::recheck_account(&root,cx); },
            "sync-issues"=>{
                if self.sync_issues.is_some() { self.sync_issues=None; }
                else if let Some(root) = self.settings_sync_root(cx) {
                    match crate::cloud_sync::issues(&root,cx) {
                        Ok(issues)=>self.sync_issues=Some(issues),
                        Err(e)=>self.show_toast_kind(ui::ToastKind::Error,&e.to_string(),cx),
                    }
                }
                cx.notify();
            },
            "plans"=>{self.subscription.show_plans=true; self.subscription_action(SubscriptionAction::Load,window,cx);},
            "purchase-annual"=>self.subscription_action(SubscriptionAction::Purchase(Plan::Annual),window,cx),
            "purchase-monthly"=>self.subscription_action(SubscriptionAction::Purchase(Plan::Monthly),window,cx),
            "purchase-restore"=>self.subscription_action(SubscriptionAction::Restore,window,cx),
            "purchase-manage"=>cx.open_url("https://apps.apple.com/account/subscriptions"),
            "trial"=>{self.confirm=Some(crate::app::Confirm::StartCloudTrial); cx.notify();},
            "share-accept"=>if !crate::cloud_restore::local_only() && !self.sharing.busy {
                self.sharing.link.update(cx,|f,cx|f.set_text(value,cx));
                self.accept_shared_note(window,cx);
            },
            "share-add"=>if let Some(room) = self.sheet_room().cloned() { self.open_invitation(room,window,cx); },
            "share-back"=>if let Some(room) = self.sheet_room().cloned() { self.open_member_management(room,window,cx); },
            "share-end"=>if let Some(room) = self.sheet_room().cloned() { self.open_end_shared(room,window,cx); },
            "share-permission"=>{self.sharing.permission=if value=="write" { Access::ReadWrite } else { Access::ReadOnly }; cx.notify();},
            "share-invite" if !value.trim().is_empty()=>{
                self.sharing.email.update(cx,|f,cx|f.set_text(value.trim(),cx)); self.submit_sharing(window,cx);
            },
            "share-audience"=>self.set_public_link(value=="link",window,cx),
            "share-public-permission"=>match value {
                "read"=>self.set_public_link_permission(Access::ReadOnly,cx),
                "write"=>self.set_public_link_permission(Access::ReadWrite,cx),
                _=>{},
            },
            "share-retry-link"=>self.retry_public_link(window,cx),
            "share-copy"=>if let Some(room) = self.sheet_room().cloned() {self.copy_shared_link(room,window,cx);},
            "share-close-link"=>self.confirm_close_link(window,cx),
            "share-cancel-close-link"=>{self.sharing.close_link_confirm=false;cx.notify();},
            "share-refresh"=>self.manage_shared_note("members",String::new(),None,window,cx),
            "share-submit"=>self.submit_sharing(window,cx),
            "share-folder-name"=>if matches!(self.sharing.sheet,Some(crate::sharing::Sheet::RenameCollection(_))) {
                self.sharing.folder_name.update(cx,|f,cx|f.set_text(value,cx));self.submit_sharing(window,cx);
            },
            "share-new-name"=>if !self.sharing.busy && matches!(self.sharing.sheet,Some(crate::sharing::Sheet::CreateNew(_,None))) {
                self.sharing.folder_name.update(cx,|f,cx|f.set_text(value.trim(),cx));cx.notify();
            },
            "share-cancel"=>self.close_sharing(window,cx),
            "shortcuts-reset"=>self.reset_all_shortcuts(cx),
            "about-website"|"about-feedback"=>if let Some((_,_,url)) = about_public_links().into_iter().find(|(link,_,_)| *link==id) {cx.open_url(url);},
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
                    if self.sync_issues.as_ref().and_then(|issues|issues.iter().find(|i|i.id==note)).is_some_and(|i|i.trashed) {
                        if self.settings_page != Page::SharedSync { self.personal_action(window,cx,|w,_,cx|w.mobile_open_trash(cx)); }
                    } else if self.is_shared() && self.settings_page != Page::SharedSync {
                        self.navigate_personal(View::Home,Some(note.into()),window,cx);
                    } else { self.open(note,window,cx); }
                }
            }
        }
    }
}
