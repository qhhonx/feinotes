import UIKit
import UserNotifications

@MainActor
final class FeiReminderNotifications: NSObject, UNUserNotificationCenterDelegate {
    struct Plan: Decodable {
        struct Item: Decodable { let identifier: String; let document: String; let title: String; let body: String; let at: Int64 }
        let namespace: String; let english: Bool; let total: Int; let requests: [Item]
    }
    struct Snapshot: Equatable {
        var authorization = UNAuthorizationStatus.notDetermined
        var pending: [String] = []
        var delivered: [String] = []
        var error: String?
        var total = 0
    }
    static let shared = FeiReminderNotifications()
    private let center = UNUserNotificationCenter.current()
    private var plan: Plan?
    private var revision: UInt64 = 0
    private var running = false
    private var backgroundTask = UIBackgroundTaskIdentifier.invalid
    private var permissionError: String?
    private var openResponse: (String,String)?
    private(set) var snapshot = Snapshot()
    var english = false
    weak var screen: FeiReminderSettings?
    func text(_ zh: String,_ en: String) -> String { english ? en : zh }
    func start() { center.delegate = self; refresh() }
    func receive(_ plan: Plan) {
        self.plan = plan; english = plan.english
        refresh()
        openPendingResponse()
    }
    func refresh() {
        revision &+= 1
        guard !running else { return }
        running = true
        Task {
            backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Reminder schedule") {
                Task { @MainActor in self.finishBackgroundTask() }
            }
            defer { running = false; finishBackgroundTask() }
            repeat {
                let current = revision
                await reconcile(current)
                if current == revision { break }
            } while true
        }
    }
    private func finishBackgroundTask() {
        guard backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundTask)
        backgroundTask = .invalid
    }
    private func reconcile(_ current: UInt64) async {
        let settings = await center.notificationSettings()
        let existing = await center.pendingNotificationRequests()
        let allowed = [.authorized,.provisional,.ephemeral].contains(settings.authorizationStatus)
        if allowed { permissionError = nil }
        let now = Date().timeIntervalSince1970*1000
        let desired = allowed ? (plan?.requests ?? []).filter { Double($0.at) > now } : []
        let wanted = Set(desired.map(\.identifier))
        let obsolete = existing.filter { $0.identifier.hasPrefix("fei.reminder.") && !wanted.contains($0.identifier) }.map(\.identifier)
        guard current == revision else { return }
        if plan != nil { center.removePendingNotificationRequests(withIdentifiers: obsolete) }
        let known = Dictionary(existing.map { ($0.identifier,$0) },uniquingKeysWith: { a,_ in a })
        var error: String?
        for item in desired {
            guard current == revision else { return }
            if let old = known[item.identifier],old.trigger is UNCalendarNotificationTrigger,old.content.title == item.title,old.content.body == item.body,old.content.userInfo["at"] as? String == String(item.at) { continue }
            let content = UNMutableNotificationContent()
            content.title = item.title; content.body = item.body; content.sound = .default
            content.userInfo = ["document":item.document,"library":plan?.namespace ?? "","at":String(item.at)]
            guard Double(item.at)/1000 > Date().timeIntervalSince1970 else { continue }
            var calendar = Calendar(identifier: .gregorian)
            calendar.timeZone = TimeZone(secondsFromGMT: 0)!
            var date = calendar.dateComponents([.year,.month,.day,.hour,.minute,.second],from: Date(timeIntervalSince1970: ceil(Double(item.at)/1000)))
            date.calendar = calendar; date.timeZone = calendar.timeZone
            let trigger = UNCalendarNotificationTrigger(dateMatching: date,repeats: false)
            do { try await center.add(UNNotificationRequest(identifier: item.identifier,content: content,trigger: trigger)) }
            catch let failure { error = failure.localizedDescription }
        }
        let pending = await center.pendingNotificationRequests()
        let delivered = await center.deliveredNotifications()
        guard current == revision else { return }
        snapshot = Snapshot(authorization: settings.authorizationStatus,pending: pending.filter { $0.identifier.hasPrefix("fei.reminder.") }.map { $0.content.title }.sorted(),delivered: delivered.filter { $0.request.identifier.hasPrefix("fei.reminder.") }.sorted { $0.date > $1.date }.map { $0.request.content.title },error: error ?? permissionError,total: plan?.total ?? 0)
        screen?.update()
    }
    func requestPermission() {
        Task {
            do { _ = try await center.requestAuthorization(options: [.alert,.sound]); permissionError = nil }
            catch { permissionError = error.localizedDescription }
            refresh()
        }
    }
    func openSystemSettings() {
        if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,willPresent notification: UNNotification,withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        completionHandler(notification.request.identifier.hasPrefix("fei.reminder.") ? [.banner,.sound,.list] : [])
        Task { @MainActor in self.refresh() }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,didReceive response: UNNotificationResponse,withCompletionHandler completionHandler: @escaping () -> Void) {
        if response.actionIdentifier == UNNotificationDefaultActionIdentifier,response.notification.request.identifier.hasPrefix("fei.reminder."),let document = response.notification.request.content.userInfo["document"] as? String,let library = response.notification.request.content.userInfo["library"] as? String {
            Task { @MainActor in self.openResponse = (document,library); self.openPendingResponse(); self.refresh() }
        }
        completionHandler()
    }
    private func openPendingResponse() {
        guard plan != nil,let (document,library) = openResponse else { return }
        openResponse = nil
        if let data = try? JSONSerialization.data(withJSONObject: ["action":"reminder-open","id":document,"value":library]),let json = String(data: data,encoding: .utf8) {
            DispatchQueue.main.async { json.withCString { fei_ios_command($0) } }
        }
    }
}

@MainActor
final class FeiReminderSettings: UITableViewController {
    private let service = FeiReminderNotifications.shared
    private var shown: FeiReminderNotifications.Snapshot?
    init() { super.init(style: .insetGrouped) }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        title = service.text("日期提醒通知","Reminder notifications")
        tableView.accessibilityIdentifier = "fei.reminder.notifications"
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 52
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: service.text("完成","Done"),style: .done,target: self,action: #selector(done))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.reminder.notifications.done"
        service.screen = self
        update()
    }
    override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); service.refresh() }
    @objc private func done() { service.screen = nil; dismiss(animated: true) }
    func update() { if shown != service.snapshot { shown = service.snapshot; if isViewLoaded { tableView.reloadData() } } }
    override func numberOfSections(in tableView: UITableView) -> Int { 3 }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { section == 0 ? 2 : section == 1 ? max(1,service.snapshot.pending.count) : max(1,service.snapshot.delivered.count) }
    override func tableView(_ tableView: UITableView,titleForHeaderInSection section: Int) -> String? {
        switch section { case 1: return service.text("已安排的日期提醒","Scheduled reminders"); case 2: return service.text("通知中心中的提醒","Reminders in Notification Center"); default: return nil }
    }
    override func tableView(_ tableView: UITableView,titleForFooterInSection section: Int) -> String? {
        if section == 0 { return service.snapshot.error ?? service.text("允许通知后，可在后台收到日期提醒。","Allow notifications to receive date reminders in the background.") }
        if section == 1 && service.snapshot.total > 60 { return service.text("已安排最近的 60 个提醒；再次打开文档库时更新后续提醒。","The next 60 reminders are scheduled. Opening the library updates later reminders.") }
        return nil
    }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .value1,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        if indexPath.section == 0 && indexPath.row == 0 {
            content.text = service.text("通知权限","Notification permission")
            switch service.snapshot.authorization {
            case .notDetermined: content.secondaryText = service.text("尚未允许","Not requested")
            case .denied: content.secondaryText = service.text("已关闭","Disabled")
            case .authorized,.provisional,.ephemeral: content.secondaryText = service.text("已允许","Allowed")
            @unknown default: content.secondaryText = service.text("暂不可用","Unavailable")
            }
            cell.accessibilityIdentifier = "fei.reminder.notifications.status"
            cell.selectionStyle = .none
        } else if indexPath.section == 0 {
            content.text = service.snapshot.authorization == .notDetermined ? service.text("允许通知","Allow notifications") : service.text("打开系统设置","Open system settings")
            content.textProperties.color = view.tintColor
            cell.accessibilityIdentifier = "fei.reminder.notifications.permission"
        } else {
            let titles = indexPath.section == 1 ? service.snapshot.pending : service.snapshot.delivered
            content.text = titles.isEmpty ? service.text("暂无提醒","No reminders") : titles[indexPath.row]
            cell.accessibilityIdentifier = indexPath.section == 1 ? "fei.reminder.notifications.pending.\(indexPath.row)" : "fei.reminder.notifications.delivered.\(indexPath.row)"
            cell.selectionStyle = .none
        }
        content.textProperties.numberOfLines = 0
        cell.contentConfiguration = content
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath,animated: true)
        guard indexPath.section == 0,indexPath.row == 1 else { return }
        if service.snapshot.authorization == .notDetermined { service.requestPermission() } else { service.openSystemSettings() }
    }
    static func present(english: Bool) {
        let service = FeiReminderNotifications.shared
        service.english = english
        DispatchQueue.main.async { "{\"action\":\"reminder-refresh\"}".withCString { fei_ios_command($0) } }
        if let screen = service.screen,screen.navigationController?.presentingViewController != nil { screen.update(); return }
        guard var host: UIViewController = EditorHost.active?.navigationController?.topViewController else { return }
        while let next = host.presentedViewController { host = next }
        host.view.endEditing(true)
        let controller = FeiReminderSettings()
        let navigation = UINavigationController(rootViewController: controller)
        navigation.sheetPresentationController?.detents = [.large()]
        navigation.sheetPresentationController?.prefersGrabberVisible = true
        host.present(navigation,animated: true)
    }
}

@_cdecl("fei_ios_reminders_changed")
@MainActor
func feiIOSRemindersChanged(_ json: UnsafePointer<CChar>) {
    guard let plan = try? JSONDecoder().decode(FeiReminderNotifications.Plan.self,from: Data(String(cString: json).utf8)) else { return }
    DispatchQueue.main.async { FeiReminderNotifications.shared.receive(plan) }
}
@_cdecl("fei_ios_present_reminder_settings")
@MainActor
func feiIOSPresentReminderSettings(_ english: Bool) { DispatchQueue.main.async { FeiReminderSettings.present(english: english) } }
