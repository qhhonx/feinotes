// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import StoreKit
import SwiftUI
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

@MainActor
private final class CloudSubscriptionSheetModel: NSObject, ObservableObject {
    let chinese: Bool
    let dark: Bool
    #if os(iOS)
    var themeAccent = UIColor.systemBlue
    #endif
    private let store: CloudSubscriptionStore?
    @Published private(set) var offers: [CloudOffer] = []
    @Published private(set) var active: CloudPlan?
    @Published private(set) var next: CloudPlan?
    @Published var selected: CloudPlan = .annual
    @Published private(set) var working = false {
        didSet { onWorkingChange?(working) }
    }
    @Published private(set) var message: String?
    @Published private(set) var messageIsError = false
    private var products: [CloudPlan: Product] = [:]
    private var accessChanged = false
    private var loaded = false
    var close: (() -> Void)?
    var onWorkingChange: ((Bool) -> Void)?
    var presenter: (() -> CloudPurchaseWindow?)?

    init(chinese: Bool, dark: Bool) {
        self.chinese = chinese
        self.dark = dark
        store = CloudSubscriptionStore(bundle: .main)
        if store == nil {
            offers = [
                CloudOffer(plan: .annual, price: "¥12.00", savingPercent: 50),
                CloudOffer(plan: .monthly, price: "¥2.00")
            ]
            loaded = true
        }
    }

    var preview: Bool { store == nil }
    var selectedOffer: CloudOffer? { offers.first { $0.plan == selected } }
    var canPurchase: Bool { !working && selectedOffer != nil && selected != active && selected != next }
    var canDismiss: Bool { !working }

    func text(_ zh: String, _ en: String) -> String { chinese ? zh : en }
    func name(_ plan: CloudPlan) -> String {
        plan == .annual ? text("年付", "Annual") : text("月付", "Monthly")
    }

    func initialLoadIfNeeded() async {
        guard !loaded else { return }
        await load()
    }

    func load() async {
        guard !working else { return }
        loaded = true
        working = true
        defer { working = false }
        setMessage(nil)
        guard let store else { return }
        do {
            products = try await store.products()
            offers = store.offers(in: products)
            if offers.isEmpty { setMessage(text("暂时无法获取订阅方案。", "Plans are temporarily unavailable."), error: true) }
        } catch {
            setMessage(text("暂时无法获取订阅方案。", "Plans are temporarily unavailable."), error: true)
        }
        await refreshStatus(store)
        selected = next ?? .annual
    }

    private func refreshStatus(_ store: CloudSubscriptionStore) async {
        let current = await store.activeSubscription()?.plan
        active = current
        if let current {
            next = try? await store.nextPlan(for: current, in: products)
        } else {
            next = nil
        }
    }

    func purchase() async {
        guard canPurchase else { return }
        let requested = selected
        setMessage(nil)
        guard let store else {
            if active == nil { active = requested }
            else { next = requested }
            return
        }
        working = true
        defer { working = false }
        do {
            let outcome = try await performCloudSubscriptionAction {
                try await store.purchase(requested, in: presenter?())
            }
            await refreshStatus(store)
            switch outcome {
            case .purchased:
                accessChanged = true
                setMessage(text("购买已确认。", "Purchase confirmed."))
            case .scheduled:
                if active != requested && next == nil { next = requested }
            case .pending:
                setMessage(text("购买正在等待批准。", "Purchase pending approval."))
            case .cancelled, .loaded, .restored:
                break
            }
        } catch StoreKitError.userCancelled {
            // Keep the selected plan and the current entitlement after cancellation.
        } catch {
            setMessage(errorMessage(error), error: true)
        }
    }

    func restore() async {
        guard !working else { return }
        setMessage(nil)
        guard let store else {
            setMessage(text("本地预览不查询 App Store 购买记录。", "The local preview does not query App Store purchases."))
            return
        }
        working = true
        defer { working = false }
        do {
            try await performCloudSubscriptionAction { try await restoreCloudSubscription() }
            await refreshStatus(store)
            accessChanged = true
            setMessage(active == nil
                ? text("当前 App Store 账户没有有效订阅。", "No active subscription for this App Store account.")
                : text("已恢复有效订阅。", "Active subscription restored."))
        } catch StoreKitError.userCancelled {
        } catch {
            setMessage(errorMessage(error), error: true)
        }
    }

    func select(_ plan: CloudPlan) {
        selected = plan
        setMessage(nil)
    }

    func finish() {
        guard presentedCloudSubscriptionSheet === self else { return }
        presentedCloudSubscriptionSheet = nil
        if accessChanged { notifyCloudSubscriptionChange() }
    }

    func previewManage() {
        setMessage(text("本地预览不会更改 App Store 订阅。", "The local preview does not change App Store subscriptions."))
    }

    private func setMessage(_ value: String?, error: Bool = false) {
        message = value
        messageIsError = error
    }

    private func errorMessage(_ error: Error) -> String {
        if let error = error as? CloudSubscriptionError {
            switch error {
            case .unavailable: return text("该方案暂不可用。", "This plan is temporarily unavailable.")
            case .missingWindow: return text("请重新打开订阅弹窗后再试。", "Reopen the subscription sheet and try again.")
            case .unverified: return text("暂时无法确认购买结果。可在 App Store 查看订阅。",
                "The purchase could not be confirmed. Check your subscription in the App Store.")
            case .busy: return text("正在处理另一笔订阅请求。", "Another subscription request is in progress.")
            }
        }
        return text("暂时无法连接 App Store，请重试。", "Could not connect to the App Store. Try again.")
    }
}

private struct CloudSubscriptionSheet: View {
    @ObservedObject var model: CloudSubscriptionSheetModel
    private var title: String { model.text("飞记云功能", "Fei Notes Cloud") }
    private var intro: String {
        #if os(macOS)
        model.text("在 Mac、iPhone 和 iPad 上接着写，也可以邀请他人一起编辑文档。",
                   "Keep writing on Mac, iPhone, and iPad. Invite others to edit with you.")
        #else
        model.text("在 Mac、iPhone 和 iPad 间同步文档，并邀请他人共同编辑。",
                   "Sync documents across Mac, iPhone, and iPad and invite others to edit together.")
        #endif
    }

    var body: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 16) {
            Text(title).font(.title2.weight(.semibold))
                .accessibilityIdentifier("fei.subscription.title")
            Text(intro).foregroundStyle(.secondary)
            if let active = model.active { accountStatus(active) }
            if model.offers.isEmpty {
                availability
            } else {
                GroupBox {
                    VStack(spacing: 0) {
                        ForEach(model.offers, id: \.plan) { offer in
                            if offer.plan != model.offers.first?.plan { Divider() }
                            planRow(offer).buttonStyle(.plain)
                        }
                    }
                } label: {
                    Text(model.text("订阅方案", "Subscription plans"))
                }
                billing
            }
            feedback
            Divider()
            HStack {
                Button(model.text("关闭", "Close")) { model.close?() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(!model.canDismiss)
                    .accessibilityIdentifier("fei.subscription.close")
                Spacer()
                if !model.offers.isEmpty { purchaseButton }
            }
            HStack { purchaseLinks; Spacer(minLength: 10); legalLinks }
                .font(.footnote)
            if model.preview { previewNotice }
        }
        .padding(24)
        .frame(width: 500)
        .fixedSize(horizontal: false, vertical: true)
        .environment(\.colorScheme, model.dark ? .dark : .light)
        .task { await model.initialLoadIfNeeded() }
        #else
        NavigationStack {
            Form {
                Section {
                    Text(intro).foregroundStyle(.secondary)
                    if let active = model.active { accountStatus(active) }
                }
                if model.offers.isEmpty {
                    Section { availability }
                } else {
                    Section(model.text("订阅方案", "Subscription plans")) {
                        ForEach(model.offers, id: \.plan) { offer in
                            planRow(offer).buttonStyle(.plain)
                        }
                    }
                    Section {
                        purchaseButton
                            .listRowBackground(Color.clear)
                            .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 0, trailing: 0))
                    } footer: {
                        billing
                    }
                }
                if model.message != nil { Section { feedback } }
                Section {
                    purchaseLinks
                    legalLinks
                } footer: {
                    if model.preview { previewNotice }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(model.text("关闭", "Close")) { model.close?() }
                        .disabled(!model.canDismiss)
                        .tint(.primary)
                        .accessibilityIdentifier("fei.subscription.close")
                }
            }
        }
        .environment(\.colorScheme, model.dark ? .dark : .light)
        .tint(Color(uiColor: model.themeAccent))
        .task { await model.initialLoadIfNeeded() }
        #endif
    }

    @ViewBuilder private var availability: some View {
        if model.working {
            ProgressView(model.text("正在查询 App Store…", "Checking the App Store…"))
        } else {
            Button(model.text("重试加载方案", "Reload plans")) { Task { await model.load() } }
        }
    }

    @ViewBuilder private var feedback: some View {
        if let message = model.message {
            Text(message)
                .font(.footnote)
                .foregroundStyle(model.messageIsError ? Color.red : Color.secondary)
                .accessibilityIdentifier("fei.subscription.message")
        }
    }

    private var previewNotice: some View {
        Text(model.text("本地预览 · 不会产生购买", "Local preview · No purchases"))
            .foregroundStyle(.secondary)
            .font(.caption)
    }

    private func accountStatus(_ active: CloudPlan) -> some View {
        let status = if let next = model.next {
            model.text("\(model.name(active))当前有效，\(model.name(next))将在下次续订时生效。",
                       "\(model.name(active)) is active. \(model.name(next)) starts at your next renewal.")
        } else {
            model.text("\(model.name(active))方案正在生效。", "Your \(model.name(active).lowercased()) plan is active.")
        }
        return Label(status, systemImage: model.next == nil ? "checkmark.circle" : "calendar")
            .font(.footnote.weight(.medium))
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("fei.subscription.status")
    }

    #if os(macOS)
    private func planRow(_ offer: CloudOffer) -> some View {
        let isSelected = model.selected == offer.plan
        let annual = offer.plan == .annual
        let price = annual ? model.text("\(offer.price) / 年", "\(offer.price) / year")
            : model.text("\(offer.price) / 月", "\(offer.price) / month")
        var details: [String] = []
        if offer.plan == model.active { details.append(model.text("当前方案", "Current plan")) }
        if offer.plan == model.next { details.append(model.text("下次续订生效", "Starts at next renewal")) }
        if let saving = offer.savingPercent { details.append(model.text("省 \(saving)%", "Save \(saving)%")) }

        return Button {
            model.select(offer.plan)
        } label: {
            HStack(alignment: .center, spacing: 10) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.name(offer.plan)).fontWeight(.medium)
                    if !details.isEmpty {
                        Text(details.joined(separator: " · "))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 4)
                Text(price)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.trailing)
                Image(systemName: "checkmark")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                    .opacity(isSelected ? 1 : 0)
                    .accessibilityHidden(true)
            }
            .padding(.vertical, 5)
            .frame(minHeight: 34)
            .contentShape(Rectangle())
        }
        .disabled(model.working)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityIdentifier("fei.subscription.plan.\(offer.plan.rawValue)")
    }

    #else
    private func planRow(_ offer: CloudOffer) -> some View {
        let isSelected = model.selected == offer.plan
        let annual = offer.plan == .annual
        let price = annual ? model.text("\(offer.price) / 年", "\(offer.price) / year")
            : model.text("\(offer.price) / 月", "\(offer.price) / month")
        return Button {
            model.select(offer.plan)
        } label: {
            HStack(alignment: .firstTextBaseline) {
                Text(model.name(offer.plan))
                if let saving = offer.savingPercent {
                    Text(model.text("省 \(saving)%", "Save \(saving)%"))
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.primary)
                        .padding(.horizontal, 5)
                        .background(Color(uiColor: model.themeAccent).opacity(0.20), in: Capsule())
                }
                Spacer(minLength: 4)
                Text(price)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.trailing)
                Image(systemName: "checkmark")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                    .opacity(isSelected ? 1 : 0)
                    .accessibilityHidden(true)
            }
            .contentShape(Rectangle())
        }
        .disabled(model.working)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityIdentifier("fei.subscription.plan.\(offer.plan.rawValue)")
    }

    private var purchaseForeground: Color {
        let style = UITraitCollection(userInterfaceStyle: model.dark ? .dark : .light)
        let accent = model.themeAccent.resolvedColor(with: style)
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0
        guard accent.getRed(&red, green: &green, blue: &blue, alpha: nil) else { return .white }
        func linear(_ value: CGFloat) -> CGFloat {
            value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
        }
        let luminance = 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
        return luminance > 0.179 ? .black : .white
    }

    private func settingsRow(_ title: String, opensAnotherPage: Bool = false) -> some View {
        HStack {
            Text(title).foregroundStyle(.primary)
            Spacer()
            if opensAnotherPage {
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
        }
        .contentShape(Rectangle())
    }
    #endif

    private var purchaseButton: some View {
        Button {
            Task { await model.purchase() }
        } label: {
            #if os(iOS)
            purchaseLabelView
                .foregroundStyle(model.canPurchase ? purchaseForeground : .secondary)
                .frame(maxWidth: .infinity)
            #else
            purchaseLabelView
            #endif
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(!model.canPurchase)
        .accessibilityIdentifier("fei.subscription.purchase")
    }

    @ViewBuilder private var purchaseLabelView: some View {
        if model.working { ProgressView().controlSize(.small) }
        else { Text(purchaseLabel).fontWeight(.semibold) }
    }

    @ViewBuilder private var billing: some View {
        if let offer = model.selectedOffer {
            Text(billingDescription(offer))
                .font(.footnote)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("fei.subscription.billing")
        }
    }

    private func billingDescription(_ offer: CloudOffer) -> String {
        if offer.plan == model.active {
            return model.text("可在 App Store 查看续订状态或管理订阅。",
                              "Check renewal status or manage your plan in the App Store.")
        }
        let renewal = offer.plan == .annual
            ? model.text("\(offer.price) 每年自动续订，可在 App Store 管理或取消。",
                         "\(offer.price) per year, auto-renewing. Manage or cancel in the App Store.")
            : model.text("\(offer.price) 每月自动续订，可在 App Store 管理或取消。",
                         "\(offer.price) per month, auto-renewing. Manage or cancel in the App Store.")
        if model.active == .monthly && offer.plan == .annual && model.next != .annual {
            return model.text("当前月付继续有效，年付预计在下次续订时生效。以 App Store 确认为准。",
                              "Monthly stays active. Annual is expected to start at your next renewal, as confirmed by the App Store.")
                + "\n" + renewal
        }
        return renewal
    }

    private var purchaseLabel: String {
        let selected = model.selected
        if selected == model.active { return model.text("当前方案", "Current plan") }
        if selected == model.next { return model.text("已安排切换\(model.name(selected))", "\(model.name(selected)) change scheduled") }
        if model.active != nil {
            return model.text("切换到\(model.name(selected))", "Switch to \(model.name(selected).lowercased())")
        }
        return model.text("通过 App Store 继续", "Continue with the App Store")
    }

    @ViewBuilder private var purchaseLinks: some View {
        Button {
            Task { await model.restore() }
        } label: {
            let title = model.text("恢复购买", "Restore purchases")
            #if os(iOS)
            settingsRow(title)
            #else
            Text(title)
            #endif
        }
        #if os(iOS)
        .buttonStyle(.plain)
        #endif
        .disabled(model.working)
        .accessibilityIdentifier("fei.subscription.restore")
        if model.active != nil {
            let title = model.text("管理订阅", "Manage subscriptions")
            if model.preview {
                Button(title) { model.previewManage() }
                    .disabled(model.working)
            } else if let url = URL(string: "https://apps.apple.com/account/subscriptions") {
                subscriptionLink(title, destination: url)
            }
        }
    }

    private func subscriptionLink(_ title: String, destination: URL) -> some View {
        #if os(iOS)
        Link(destination: destination) { settingsRow(title, opensAnotherPage: true) }
            .buttonStyle(.plain)
        #else
        Link(title, destination: destination)
        #endif
    }

    @ViewBuilder private var legalLinks: some View {
        legalLink("FeiPrivacyURL", model.text("隐私政策", "Privacy policy"),
                  previewURL: "https://feinotes.app/privacy/")
        legalLink("FeiTermsURL", model.text("使用条款", "Terms of use"),
                  previewURL: "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/")
    }

    @ViewBuilder private func legalLink(_ key: String, _ label: String, previewURL: String) -> some View {
        let configured = Bundle.main.object(forInfoDictionaryKey: key) as? String
        let valid = configured.flatMap { URL(string: $0) }.flatMap { $0.scheme == "https" ? $0 : nil }
        if let url = valid ?? (model.preview ? URL(string: previewURL) : nil) {
            subscriptionLink(label, destination: url)
                .accessibilityIdentifier("fei.subscription.legal.\(key)")
        }
    }
}

@MainActor private weak var presentedCloudSubscriptionSheet: CloudSubscriptionSheetModel?

@_cdecl("quiet_cloud_subscription_sheet_available")
public func quietCloudSubscriptionSheetAvailable() -> Bool {
    if quietCloudSubscriptionAvailable() { return true }
    #if FEI_CLOUD_QA
    return Bundle.main.bundleIdentifier == "app.quietmd.cloudlab"
        && Bundle.main.object(forInfoDictionaryKey: "FeiLocalPreview") as? Bool == true
    #else
    return false
    #endif
}

@_cdecl("quiet_cloud_show_subscription_sheet")
public func quietCloudShowSubscriptionSheet(_ chinese: Bool, _ dark: Bool, _ red: Float, _ green: Float, _ blue: Float) {
    Task { @MainActor in
        guard quietCloudSubscriptionSheetAvailable(), presentedCloudSubscriptionSheet == nil else { return }
        let model = CloudSubscriptionSheetModel(chinese: chinese, dark: dark)
        #if os(macOS)
        let view = CloudSubscriptionSheet(model: model)
        guard let owner = NSApp.keyWindow, owner.attachedSheet == nil else { return }
        let host = NSHostingController(rootView: view)
        let sheet = NSWindow(contentViewController: host)
        sheet.titleVisibility = .hidden
        sheet.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        sheet.setContentSize(host.view.fittingSize)
        model.presenter = { [weak sheet] in sheet }
        model.close = { [weak owner, weak sheet] in
            if let sheet { owner?.endSheet(sheet) }
        }
        presentedCloudSubscriptionSheet = model
        owner.beginSheet(sheet) { _ in model.finish() }
        #elseif os(iOS)
        model.themeAccent = UIColor(red: CGFloat(red), green: CGFloat(green), blue: CGFloat(blue), alpha: 1)
        let view = CloudSubscriptionSheet(model: model)
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive && $0.keyWindow != nil }
        var owner = scene?.keyWindow?.rootViewController
        while let presented = owner?.presentedViewController { owner = presented }
        guard let owner, !owner.isBeingDismissed else { return }
        let host = UIHostingController(rootView: view)
        host.overrideUserInterfaceStyle = dark ? .dark : .light
        let iPad = owner.traitCollection.userInterfaceIdiom == .pad
        host.modalPresentationStyle = iPad ? .formSheet : .pageSheet
        host.preferredContentSize = CGSize(width: 530, height: 540)
        if !iPad { host.sheetPresentationController?.detents = [.large()] }
        host.presentationController?.delegate = model
        model.presenter = { [weak host] in host }
        model.onWorkingChange = { [weak host] working in host?.isModalInPresentation = working }
        model.close = { [weak host, weak model] in
            guard let host, let model else { return }
            host.dismiss(animated: true) { model.finish() }
        }
        presentedCloudSubscriptionSheet = model
        owner.present(host, animated: true)
        #endif
    }
}

#if os(iOS)
extension CloudSubscriptionSheetModel: UIAdaptivePresentationControllerDelegate {
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { finish() }
}
#endif
