// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use super::tr;
use crate::{app::{SaveStatus, View, Workspace}, cloud_sync::{self, SyncPhase}};
use gpui::App;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action { Retry, Account, Plans }

#[derive(serde::Serialize)]
pub(crate) struct Status {
    pub title: &'static str,
    pub scope: String,
    pub detail: &'static str,
    pub rows: Vec<(&'static str, String)>,
    pub attention: bool,
    pub action: Option<Action>,
}

pub(crate) fn marker_colors() -> (gpui::Hsla, gpui::Hsla) {
    use gpui::rgb;
    // Colors from the approved sync badge in each built-in theme.
    match (super::feishu(), crate::theme::dark()) {
        (true, false) => (rgb(0x9a4a00).into(), gpui::white()),
        (false, false) => (rgb(0xa3412b).into(), gpui::white()),
        (true, true) => (rgb(0xf4b56b).into(), rgb(0x241813).into()),
        (false, true) => (rgb(0xf1a184).into(), rgb(0x241813).into()),
    }
}

fn local_save(workspace: &Workspace) -> Option<bool> {
    if workspace.view != View::Doc || workspace.current.is_none() { return None; }
    if matches!(workspace.status, SaveStatus::Failed(_)) { return Some(false); }
    (workspace.unsaved || workspace.status == SaveStatus::Saving).then_some(true)
}

pub(crate) fn needs_attention(workspace: &Workspace, cx: &App) -> bool {
    if !workspace.is_shared() { return false; }
    if let Some(saving) = local_save(workspace) { return !saving; }
    cloud_sync::status(workspace.store.root(), cx).is_some_and(|s| {
        matches!(s.phase, SyncPhase::AccountChanged | SyncPhase::SignedOut |
            SyncPhase::SubscriptionRequired | SyncPhase::ReadOnly | SyncPhase::Revoked)
            || s.access_paused && !matches!(s.phase, SyncPhase::Account | SyncPhase::AccountChecking | SyncPhase::TestFlightVerification)
            || !s.retry_requested && (s.phase == SyncPhase::Quota || s.stalled.is_some() || s.held > 0 || s.attachments > 0 || s.conflicts > 0 || s.catalogue_paused)
    })
}

pub(crate) fn shared(workspace: &Workspace, cx: &App) -> Option<Status> {
    let room = workspace.current_room(cx)?;
    let scope = crate::model::display_title(&room.title);
    let mut card = Status { title: tr("正在连接 iCloud", "Connecting to iCloud"), scope, detail: "", rows: vec![],
        attention: needs_attention(workspace, cx), action: None };
    if let Some(saving) = local_save(workspace) {
        (card.title, card.detail) = if saving {
            (tr("正在保存到本机", "Saving on this device"), tr("保存完成后继续同步。", "Sync resumes after saving."))
        } else {
            (tr("本机保存失败", "Couldn’t save on this device"), tr("请回到文档重试保存。", "Return to the document to retry saving."))
        };
        return Some(card);
    }
    let root = workspace.store.root();
    let Some(status) = cloud_sync::status(root,cx) else { return Some(card); };
    use SyncPhase::*;
    let account_check = matches!(status.phase, Account | AccountChecking | AccountChanged | SignedOut | TestFlightVerification);
    let phase = if status.access_paused && !account_check && status.phase != Revoked { SubscriptionRequired } else { status.phase };
    let (title, detail, action) = match phase {
        SignedOut => (tr("iCloud 已退出登录", "Signed out of iCloud"), tr("请登录原 iCloud 账户。已保存的改动会保留。", "Sign in to your previous iCloud account. Saved changes are kept."), Some(Action::Account)),
        AccountChanged => (tr("iCloud 账户已切换", "iCloud account changed"), tr("原账户的待同步改动会保留。", "Pending changes for the previous account are kept."), Some(Action::Account)),
        Account | AccountChecking => (tr("正在确认 iCloud 账户", "Checking iCloud account"), "", None),
        TestFlightVerification => (tr("正在验证 TestFlight 安装", "Verifying TestFlight installation"), "", None),
        SubscriptionRequired => (tr("同步已暂停", "Sync paused"), crate::cloud_trial::access_required(), Some(Action::Plans)),
        Revoked => (tr("共享访问已结束", "Sharing access ended"), tr("需要继续访问时，请联系所有者。", "Contact the owner if you still need access."), None),
        ReadOnly => (tr("共享权限已改为只读", "Sharing access is now read-only"), tr("请联系所有者恢复编辑权限。", "Ask the owner to restore editing access."), None),
        _ if status.retry_requested => (tr("正在尝试同步", "Retrying sync"), tr("已请求重试，等待 iCloud 响应。", "A retry was requested. Waiting for iCloud."), None),
        Quota => (tr("iCloud 空间不足", "iCloud storage is full"), tr("释放空间后可重试同步。", "Free up space, then retry sync."), Some(Action::Retry)),
        _ if status.stalled.is_some() => match status.stalled.unwrap_or(phase) {
            Uploading => (tr("上传停滞", "Upload stalled"), tr("改动已保存在本机。检查网络和 iCloud 空间后可重试。", "Changes are saved here. Check your connection and iCloud storage before retrying."), Some(Action::Retry)),
            Applying => (tr("应用更新停滞", "Applying updates stalled"), tr("请检查本机的可用空间。", "Check available storage on this device."), Some(Action::Retry)),
            _ => (tr("同步停滞", "Sync stalled"), tr("请检查网络后重试。", "Check your connection before retrying."), Some(Action::Retry)),
        },
        Offline | Failed => (tr("等待自动重试", "Waiting to retry"), if status.uploads > 0 {
            tr("iCloud 暂时不可用。待上传改动已保存在本机。", "iCloud is temporarily unavailable. Pending changes are saved here.")
        } else { tr("暂时无法连接 iCloud。", "iCloud is temporarily unavailable.") }, None),
        Uploading => (tr("正在上传", "Uploading"), tr("待上传改动已保存在本机。", "Pending changes are saved on this device."), None),
        Downloading => (tr("正在下载更新", "Downloading updates"), "", None),
        Applying => (tr("正在应用更新", "Applying updates"), "", None),
        Connecting => (card.title, "", None),
        Ready if status.conflicts > 0 || status.held > 0 || status.attachments > 0 || status.catalogue_paused =>
            (tr("部分内容未同步", "Some items couldn’t sync"), tr("请检查连接和附件后重试。", "Check your connection and attachments before retrying."), Some(Action::Retry)),
        Ready if status.downloads > 0 => (tr("正在应用更新", "Applying updates"), "", None),
        Ready if status.uploads > 0 => (tr("等待上传", "Waiting to upload"), "", None),
        Ready => (tr("已同步", "Up to date"), "", None),
    };
    card.title = title; card.detail = detail; card.action = action;
    if !account_check && !matches!(phase, Revoked | SubscriptionRequired) {
        if status.uploads > 0 { card.rows.push((tr("待上传", "Pending uploads"), status.uploads.to_string())); }
        if status.downloads > 0 { card.rows.push((tr("待应用到本机", "Waiting to apply here"), status.downloads.to_string())); }
        if let Some(time) = cloud_sync::last_success(root, cx) {
            let (y, m, d, h, min) = crate::time::civil(time);
            card.rows.push((tr("上次同步成功", "Last successful sync"), format!("{y}-{m:02}-{d:02} {h:02}:{min:02}")));
        }
    }
    Some(card)
}
