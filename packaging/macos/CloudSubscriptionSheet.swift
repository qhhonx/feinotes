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
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        #if os(macOS)
        content
            .frame(width: 512)
            .fixedSize(horizontal: false, vertical: true)
            .onExitCommand { if model.canDismiss { model.close?() } }
            .environment(\.colorScheme, model.dark ? .dark : .light)
            .task { await model.initialLoadIfNeeded() }
        #else
        ScrollView {
            content
                .frame(maxWidth: 480)
                .frame(maxWidth: .infinity)
        }
        .background(Color(uiColor: .systemBackground))
        .environment(\.colorScheme, model.dark ? .dark : .light)
        .task { await model.initialLoadIfNeeded() }
        #endif
    }

    private var content: some View {
        VStack(spacing: 16) {
            heading
            if let active = model.active { accountStatus(active) }
            if model.offers.isEmpty {
                if model.working {
                    ProgressView(model.text("正在查询 App Store…", "Checking the App Store…"))
                        .frame(minHeight: 65)
                } else {
                    Button(model.text("重试加载方案", "Reload plans")) { Task { await model.load() } }
                        .frame(minHeight: 44)
                }
            } else {
                planChoices
                payment
            }
            if let message = model.message {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(model.messageIsError ? Color.red : Color.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("fei.subscription.message")
            }
            footer
        }
        .padding(24)
    }

    private var heading: some View {
        VStack(spacing: 7) {
            Image(systemName: "icloud")
                .font(.system(size: 23, weight: .medium))
                .foregroundStyle(Color.accentColor)
                .frame(width: 43, height: 43)
                .background(Color.accentColor.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
                .accessibilityHidden(true)
            Text(model.text("飞记云功能", "Fei Notes Cloud"))
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("fei.subscription.title")
            Text(model.text("在 Mac、iPhone 和 iPad 上接着写，也可以邀请他人一起编辑文档。",
                            "Keep writing on Mac, iPhone, and iPad. Invite others to edit with you."))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 330)
        }
        .frame(maxWidth: .infinity)
        .overlay(alignment: .topTrailing) {
            Button { model.close?() } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 36, height: 36)
                    .background(Color.primary.opacity(0.065), in: Circle())
            }
            .buttonStyle(.plain)
            .disabled(!model.canDismiss)
            .accessibilityLabel(model.text("关闭订阅弹窗", "Close subscription sheet"))
            .accessibilityIdentifier("fei.subscription.close")
        }
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
            .foregroundStyle(Color.accentColor)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(10)
            .background(Color.accentColor.opacity(0.10), in: RoundedRectangle(cornerRadius: 8))
            .accessibilityIdentifier("fei.subscription.status")
    }

    @ViewBuilder private var planChoices: some View {
        if typeSize.isAccessibilitySize {
            VStack(spacing: 9) {
                ForEach(model.offers, id: \.plan) { offer in planCard(offer) }
            }
        } else {
            HStack(alignment: .top, spacing: 9) {
                ForEach(model.offers, id: \.plan) { offer in planCard(offer) }
            }
        }
    }

    private func planCard(_ offer: CloudOffer) -> some View {
        let isSelected = model.selected == offer.plan
        let isAnnual = offer.plan == .annual
        let badge = model.active == offer.plan ? model.text("当前方案", "Current plan")
            : model.next == offer.plan ? model.text("下次续订生效", "Starts at next renewal") : nil
        return Button {
            model.select(offer.plan)
        } label: {
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Text(model.name(offer.plan)).font(.subheadline.weight(.semibold))
                    Spacer(minLength: 2)
                    Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                        .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                        .accessibilityHidden(true)
                }
                (
                    Text(offer.price).font(.title3.weight(.semibold))
                    + Text(isAnnual ? model.text(" / 年", " / year") : model.text(" / 月", " / month")).font(.footnote)
                )
                .foregroundStyle(.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
                if let saving = offer.savingPercent {
                    Text(model.text("省 \(saving)%", "Save \(saving)%"))
                        .foregroundStyle(Color.accentColor)
                        .font(.footnote.weight(.medium))
                } else {
                    Text(isAnnual ? model.text("每年付款", "Billed yearly") : model.text("每月付款", "Billed monthly"))
                        .foregroundStyle(.secondary)
                        .font(.footnote)
                }
                if model.active != nil && !typeSize.isAccessibilitySize || badge != nil {
                    Text(badge ?? " ")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Color.accentColor)
                        .accessibilityHidden(badge == nil)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(isSelected ? Color.accentColor.opacity(0.08) : Color.clear, in: RoundedRectangle(cornerRadius: 11))
            .overlay {
                RoundedRectangle(cornerRadius: 11)
                    .strokeBorder(isSelected ? Color.accentColor : Color.secondary.opacity(0.28), lineWidth: 1)
            }
            .contentShape(RoundedRectangle(cornerRadius: 11))
        }
        .buttonStyle(.plain)
        .disabled(model.working)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityIdentifier("fei.subscription.plan.\(offer.plan.rawValue)")
    }

    private var payment: some View {
        VStack(spacing: 6) {
            Button {
                Task { await model.purchase() }
            } label: {
                Group {
                    if model.working { ProgressView().controlSize(.small) }
                    else { Text(purchaseLabel).fontWeight(.semibold) }
                }
                .frame(maxWidth: .infinity, minHeight: 30)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(!model.canPurchase)
            .accessibilityIdentifier("fei.subscription.purchase")

            if let offer = model.selectedOffer {
                Text(billingDescription(offer))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .accessibilityIdentifier("fei.subscription.billing")
            }
        }
    }

    private func billingDescription(_ offer: CloudOffer) -> String {
        if offer.plan == model.active {
            return model.text("可在 App Store 查看续订状态或管理订阅。",
                              "Check renewal status or manage your plan in the App Store.")
        }
        return offer.plan == .annual
            ? model.text("\(offer.price) 每年自动续订，可在 App Store 管理或取消。",
                         "\(offer.price) per year, auto-renewing. Manage or cancel in the App Store.")
            : model.text("\(offer.price) 每月自动续订，可在 App Store 管理或取消。",
                         "\(offer.price) per month, auto-renewing. Manage or cancel in the App Store.")
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

    private var footer: some View {
        VStack(spacing: 8) {
            Divider()
            ViewThatFits(in: .horizontal) {
                HStack { purchaseLinks; Spacer(minLength: 10); legalLinks }
                VStack(spacing: 8) { purchaseLinks; legalLinks }
            }
            if model.preview {
                Text(model.text("本地预览 · 不会产生购买", "Local preview · No purchases"))
                    .foregroundStyle(.secondary)
                    .font(.caption)
            }
        }
        .font(.footnote)
    }

    private var purchaseLinks: some View {
        HStack(spacing: 12) {
            Button(model.text("恢复购买", "Restore purchases")) { Task { await model.restore() } }
                .accessibilityIdentifier("fei.subscription.restore")
                .foregroundStyle(Color.accentColor)
            if model.active != nil {
                if model.preview {
                    Button(model.text("管理订阅", "Manage subscriptions")) { model.previewManage() }
                        .foregroundStyle(Color.accentColor)
                } else if let url = URL(string: "https://apps.apple.com/account/subscriptions") {
                    Link(model.text("管理订阅", "Manage subscriptions"), destination: url)
                }
            }
        }
        .buttonStyle(.borderless)
        .disabled(model.working)
    }

    private var legalLinks: some View {
        HStack(spacing: 12) {
            legalLink("FeiPrivacyURL", model.text("隐私政策", "Privacy policy"),
                      previewURL: "https://feinotes.app/privacy/")
            legalLink("FeiTermsURL", model.text("使用条款", "Terms of use"),
                      previewURL: "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/")
        }
        .buttonStyle(.borderless)
    }

    @ViewBuilder private func legalLink(_ key: String, _ label: String, previewURL: String) -> some View {
        let configured = Bundle.main.object(forInfoDictionaryKey: key) as? String
        let valid = configured.flatMap { URL(string: $0) }.flatMap { $0.scheme == "https" ? $0 : nil }
        if let url = valid ?? (model.preview ? URL(string: previewURL) : nil) {
            Link(label, destination: url)
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
public func quietCloudShowSubscriptionSheet(_ chinese: Bool, _ dark: Bool) {
    Task { @MainActor in
        guard quietCloudSubscriptionSheetAvailable(), presentedCloudSubscriptionSheet == nil else { return }
        let model = CloudSubscriptionSheetModel(chinese: chinese, dark: dark)
        let view = CloudSubscriptionSheet(model: model)
        #if os(macOS)
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
