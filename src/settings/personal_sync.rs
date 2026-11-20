use super::tr;
use crate::cloud_sync::{SyncPhase, SyncStatus};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action { Sync, AutoRetry, VerifyAccount, SignIn, ChangedAccount, Plans, None }

pub(super) struct Status {
    pub title: &'static str,
    pub detail: Option<&'static str>,
    pub rows: Vec<(&'static str, &'static str, String)>,
    pub action: Action,
}

pub(super) fn read(root: &Path, cx: &gpui::App) -> Status {
    present(crate::cloud_sync::status(root, cx), crate::cloud_sync::account_is_current(root, cx),
        crate::cloud_sync::last_success(root, cx))
}

fn present(sync: Option<&SyncStatus>, current_account: bool, last_success: Option<u64>) -> Status {
    use SyncPhase::*;
    let empty = SyncStatus::default();
    let sync = sync.unwrap_or(&empty);
    let phase = if !current_account && sync.access_verified
        && !matches!(sync.phase, SignedOut | AccountChanged | Account) { AccountChecking } else { sync.phase };
    let (title, detail, action) = match phase {
        SignedOut => (tr("iCloud 已退出登录", "Signed out of iCloud"), Some(tr(
            "请登录原 iCloud 账户。文档和待上传改动会保留。",
            "Sign in to your previous iCloud account. Documents and pending uploads are kept.")), Action::SignIn),
        AccountChanged => (tr("iCloud 账户已切换", "iCloud account changed"), Some(tr(
            "原文档库和待上传改动会保留。可登录原账户恢复同步，或关闭同步后使用本机文档库。",
            "Your previous library and pending uploads are kept. Sign back in to resume, or turn off sync to use your local library.")), Action::ChangedAccount),
        AccountChecking => (tr("正在确认 iCloud 账户", "Checking the iCloud account"), Some(tr(
            "验证后会继续同步。已保存的改动会保留。",
            "Sync resumes after verification. Saved changes are kept.")), Action::None),
        Account => (tr("暂时无法确认 iCloud 账户", "Could not verify the iCloud account"), Some(tr(
            "飞记会自动重试。也可以现在重新验证。",
            "Fei Notes will retry automatically. You can also verify now.")), Action::VerifyAccount),
        _ if sync.access_paused || phase == SubscriptionRequired => (tr("同步已暂停", "Sync paused"), Some(tr(
            "当前没有有效的云功能权益。文档和待上传改动会保留。",
            "Cloud access is currently inactive. Documents and pending uploads are kept.")), Action::Plans),
        TestFlightVerification => (tr("正在验证 TestFlight 安装", "Verifying the TestFlight installation"), Some(tr(
            "请保持联网。改动已保存在本机。",
            "Stay connected to the internet. Changes are kept on this device.")), Action::Sync),
        Offline | Failed => (tr("等待自动重试", "Waiting to retry automatically"), Some(tr(
            "iCloud 暂时不可用。已保存的文档可以继续使用。",
            "iCloud is temporarily unavailable. You can keep using saved documents.")), Action::AutoRetry),
        Quota => (tr("iCloud 空间不足", "iCloud storage is full"), Some(tr(
            "腾出空间后可点「立即同步」。", "Free up iCloud storage, then choose Sync Now.")), Action::Sync),
        _ => {
            let stalled = sync.stalled.filter(|phase| matches!(phase, Connecting | Uploading | Downloading | Applying));
            let title = match stalled.unwrap_or(phase) {
                Connecting if stalled.is_some() => tr("连接停滞", "Connection stalled"),
                Uploading if stalled.is_some() => tr("上传停滞", "Uploads stalled"),
                Downloading if stalled.is_some() => tr("下载停滞", "Downloads stalled"),
                Applying if stalled.is_some() => tr("应用改动停滞", "Applying changes stalled"),
                Connecting => tr("正在连接 iCloud", "Connecting to iCloud"),
                Uploading => tr("正在上传改动", "Uploading changes"),
                Downloading => tr("正在下载改动", "Downloading changes"),
                Applying => tr("正在应用改动", "Applying changes"),
                Ready if sync.catalogue_paused => tr("文档目录同步已暂停", "Document list sync paused"),
                Ready if sync.conflicts > 0 => tr("部分文档需要恢复", "Some documents need recovery"),
                Ready if sync.downloads > 0 => tr("正在应用改动", "Applying changes"),
                Ready if sync.uploads > 0 => tr("正在等待上传", "Waiting to upload"),
                Ready if sync.held > 0 || sync.attachments > 0 => tr("部分内容未同步", "Some items couldn’t sync"),
                Ready if last_success.is_some() => tr("已同步", "Up to date"),
                _ => tr("正在同步", "Syncing"),
            };
            let detail = if title == tr("已同步", "Up to date") {
                Some(tr("这台设备的待上传改动已获云端确认。", "iCloud has confirmed this device’s pending changes."))
            } else if sync.catalogue_paused {
                Some(tr("已保留目录改动，已有文档的正文仍可同步。", "Document list changes are kept. Existing document contents can still sync."))
            } else if phase == Applying && sync.apply_wait.local_edit {
                Some(tr("正在等待本机编辑保存。", "Waiting for local edits to save."))
            } else if stalled == Some(Applying) && sync.apply_wait.local_error {
                Some(tr("本机暂时无法应用改动，待处理改动仍保留。", "This device couldn't apply the changes yet. Pending changes are kept."))
            } else if stalled == Some(Applying) && sync.apply_wait.catalogue_predecessor {
                Some(tr("部分目录改动缺少历史记录，已保留。其他改动仍可同步。", "Some changes to the document list need earlier records and are kept. Other changes can still sync."))
            } else if stalled == Some(Applying) &&
                (sync.apply_wait.documents_awaiting_predecessor > 0 || sync.apply_wait.documents_awaiting_catalogue > 0) {
                Some(tr("部分改动依赖尚未同步的早期改动。可点「立即同步」重试。", "Some changes need earlier changes. Choose Sync Now to retry."))
            } else if stalled == Some(Applying) && sync.apply_wait.documents_awaiting_attachment > 0 {
                Some(tr("部分改动正在等待附件。可点「立即同步」重试。", "Some changes are waiting for attachments. Choose Sync Now to retry."))
            } else if stalled == Some(Applying) && sync.downloads > 0 {
                Some(tr("已有改动等待在这台设备上应用。可点「立即同步」重试。", "Changes are waiting to be applied on this device. Choose Sync Now to retry."))
            } else if stalled.is_some() {
                Some(tr("可检查网络和可用空间，再点「立即同步」。", "Check your connection and available storage, then choose Sync Now."))
            } else { None };
            (title, detail, if sync.access_verified || stalled.is_some() { Action::Sync } else { Action::None })
        }
    };
    let account = match phase {
        SignedOut => tr("已退出登录", "Signed out"),
        AccountChanged => tr("与原文档库账户不同", "Different from the previous library"),
        AccountChecking => tr("正在验证", "Verifying"),
        Account => tr("暂时无法确认", "Could not verify"),
        _ if current_account && (sync.access_verified || phase == SubscriptionRequired) => tr("已连接", "Connected"),
        _ => tr("待验证", "Awaiting verification"),
    };
    let access = match action {
        Action::Plans => tr("无有效权益", "No active access"),
        _ if current_account && sync.access_verified && matches!(phase, Offline | Failed) =>
            tr("上次验证有效", "Active when last verified"),
        _ if current_account && sync.access_verified => tr("有效", "Active"),
        _ => tr("待验证", "Awaiting verification"),
    };
    let success = last_success.map(|time| {
        let (y,m,d,h,min) = crate::time::civil(time);
        format!("{y}-{m:02}-{d:02} {h:02}:{min:02}")
    }).unwrap_or_else(|| {
        if matches!(phase, AccountChecking | AccountChanged | SignedOut | Account) { tr("待确认账户", "Awaiting account verification") }
        else { tr("暂无记录", "No record yet") }.into()
    });
    let mut rows = vec![
        ("sync-last-success", tr("上次同步成功", "Last successful sync"), success),
        ("sync-account", tr("iCloud 账户", "iCloud account"), account.into()),
        ("sync-access", tr("云功能权益", "Cloud access"), access.into()),
    ];
    if sync.uploads > 0 || sync.downloads > 0 {
        rows.insert(0, ("sync-uploads", tr("待上传（已保存在本机）", "Pending uploads (saved on this device)"), sync.uploads.to_string()));
        rows.insert(1, ("sync-downloads", tr("本机待应用", "Pending on this device"), sync.downloads.to_string()));
    }
    Status { title, detail, rows, action }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_details_require_observed_signals() {
        use SyncPhase::*;
        let pending = present(None, false, None);
        assert_eq!(pending.action, Action::None);
        let mut status = SyncStatus::default();
        status.phase = Ready;
        status.access_verified = true;
        assert_ne!(present(Some(&status), true, None).title, tr("已同步", "Up to date"));
        assert_eq!(present(Some(&status), true, Some(3)).title, tr("已同步", "Up to date"));
        let checking = present(Some(&status), false, None);
        assert_eq!(checking.action, Action::None);
        assert_eq!(checking.rows[2].2, tr("待验证", "Awaiting verification"));
        status.phase = Offline;
        status.uploads = 2;
        let offline = present(Some(&status), true, Some(3));
        assert_eq!(offline.action, Action::AutoRetry);
        assert_eq!(offline.rows[0].2, "2");
        status.phase = SignedOut;
        assert_eq!(present(Some(&status), false, None).action, Action::SignIn);
        status.phase = AccountChanged;
        assert_eq!(present(Some(&status), false, None).action, Action::ChangedAccount);
        status.phase = SubscriptionRequired;
        status.access_verified = false;
        assert_eq!(present(Some(&status), false, None).action, Action::Plans);
    }
}
