import UIKit

@MainActor
final class FeiTextActions: NSObject, UIEditMenuInteractionDelegate, UIGestureRecognizerDelegate {
    struct Request: Decodable {
        let serial: UInt64
        let x: CGFloat
        let y: CGFloat
        let rect: [CGFloat]
        let handles: [[CGFloat]]
        let editable: Bool
        let selected: Bool
        let labels: [String: String]
    }
    static let shared = FeiTextActions()
    private weak var source: UIView?
    private var interaction: UIEditMenuInteraction?
    private var request: Request?
    private weak var panView: UIView?
    private var selectionPan: UIPanGestureRecognizer?
    private var presentationGeneration: UInt64 = 0

    func enqueue(_ request: Request) {
        presentationGeneration &+= 1
        let generation = presentationGeneration
        DispatchQueue.main.async { [weak self] in
            guard let self,self.presentationGeneration == generation else { return }
            self.present(request)
        }
    }

    func present(_ request: Request) {
        guard let window = gpui_ios_get_window(),let pointer = gpui_ios_view_controller(window) else { return }
        let controller = Unmanaged<UIViewController>.fromOpaque(pointer).takeUnretainedValue()
        guard let view = controller.viewIfLoaded,view.window != nil else { return }
        if panView !== view {
            if let selectionPan { panView?.removeGestureRecognizer(selectionPan) }
            let pan = UIPanGestureRecognizer(target: self,action: #selector(reserveSelectionDrag))
            pan.maximumNumberOfTouches = 1
            pan.cancelsTouchesInView = false
            pan.delaysTouchesBegan = false
            pan.delaysTouchesEnded = false
            pan.delegate = self
            view.addGestureRecognizer(pan)
            selectionPan = pan
            panView = view
        }
        if source !== view {
            if let interaction { source?.removeInteraction(interaction) }
            let interaction = UIEditMenuInteraction(delegate: self)
            view.addInteraction(interaction)
            self.interaction = interaction
            source = view
        }
        self.request = request
        let point = CGPoint(x: max(1,min(request.x,view.bounds.width-1)),y: max(1,min(request.y,view.bounds.height-1)))
        let config = UIEditMenuConfiguration(identifier: NSNumber(value: request.serial),sourcePoint: point)
        config.preferredArrowDirection = .down
        interaction?.presentEditMenu(with: config)
    }

    func dismiss() {
        presentationGeneration &+= 1
        interaction?.dismissMenu()
        if let interaction { source?.removeInteraction(interaction) }
        interaction = nil
        source = nil
        request = nil
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,shouldReceive touch: UITouch) -> Bool {
        guard gestureRecognizer === selectionPan,let request,let panView else { return false }
        let point = touch.location(in: panView)
        return point.x >= 8 && request.handles.contains { handle in
            handle.count == 2 && hypot(point.x-handle[0],point.y-handle[1]) <= 22
        }
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,shouldBeRequiredToFailBy otherGestureRecognizer: UIGestureRecognizer) -> Bool {
        gestureRecognizer === selectionPan
    }

    @objc private func reserveSelectionDrag(_ gesture: UIPanGestureRecognizer) {
        // Recognition reserves the raw contact for GPUI; Core owns the drag.
    }

    func editMenuInteraction(_ interaction: UIEditMenuInteraction,targetRectFor configuration: UIEditMenuConfiguration) -> CGRect {
        guard let request,request.rect.count == 4,let source else { return .null }
        let rect = CGRect(x: request.rect[0],y: request.rect[1],width: request.rect[2],height: request.rect[3]).intersection(source.bounds)
        return rect.isNull ? CGRect(origin: configuration.sourcePoint,size: CGSize(width: 1,height: 1)) : rect
    }

    func editMenuInteraction(_ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration,
        suggestedActions: [UIMenuElement]) -> UIMenu? {
        guard let request,configuration.identifier as? NSNumber == NSNumber(value: request.serial) else { return nil }
        var keys: [String] = []
        if request.selected {
            if request.editable { keys.append("cut") }
            keys.append("copy")
        }
        if request.editable { keys.append("paste") }
        keys.append("select-all")
        if request.selected && request.editable { keys.append("format") }
        return UIMenu(children: keys.map { key in
            UIAction(title: request.labels[key] ?? key) { _ in
                let command: [String: Any] = ["action":"text-action","value":key,"selection_serial":request.serial]
                guard let data = try? JSONSerialization.data(withJSONObject: command),let json = String(data: data,encoding: .utf8) else { return }
                json.withCString { fei_ios_command($0) }
            }
        })
    }
}

@_cdecl("fei_ios_present_text_menu")
@MainActor
func feiIOSPresentTextMenu(_ json: UnsafePointer<CChar>) {
    let data = Data(String(cString: json).utf8)
    guard let request = try? JSONDecoder().decode(FeiTextActions.Request.self,from: data) else { return }
    FeiTextActions.shared.enqueue(request)
}

@_cdecl("fei_ios_dismiss_text_menu")
@MainActor
func feiIOSDismissTextMenu() { FeiTextActions.shared.dismiss() }

@_cdecl("fei_ios_clipboard_html")
@MainActor
func feiIOSClipboardHTML() -> UnsafeMutablePointer<CChar>? {
    guard let data = UIPasteboard.general.data(forPasteboardType: "public.html"),data.count <= 16 * 1024 * 1024,
          let html = String(data: data,encoding: .utf8) else { return nil }
    return strdup(html)
}
