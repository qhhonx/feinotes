import UIKit

@MainActor
final class FeiEditorToolbar: UIToolbar {
    /// A fixed Latin Aa avoids the localized CJK textformat symbol variants.
    static var formatIcon: UIImage {
        let size = CGSize(width: 26,height: 24)
        let font = UIFont.systemFont(ofSize: 20,weight: .regular)
        return UIGraphicsImageRenderer(size: size).image { _ in
            let text = "Aa" as NSString
            let bounds = text.size(withAttributes: [.font:font])
            text.draw(at: CGPoint(x:(size.width-bounds.width)/2,y:(size.height-bounds.height)/2),withAttributes: [.font:font,.foregroundColor:UIColor.label])
        }.withRenderingMode(.alwaysTemplate)
    }

    static let preferredHeight: CGFloat = 52
    private weak var coordinator: FeiNavigationCoordinator?
    private var state: FeiNavigationState?
    private var mode: String?
    private var composeAllowed = false
    private var reducedTools = false
    private var labels: [String:String] = [:]
    private var keyboardVisible = false
    private let keyboardButton = UIBarButtonItem()
    private var tableButton: UIBarButtonItem?
    private var commandButtons: [UIBarButtonItem] = []
    private var typing = false
    private(set) var navigationItems: [UIBarButtonItem] = []

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        accessibilityIdentifier = "fei.editor.toolbar"
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    private func spacer() -> UIBarButtonItem { UIBarButtonItem(barButtonSystemItem: .flexibleSpace,target: nil,action: nil) }
    private func button(_ symbol:String,_ key:String,_ id:String,_ selector:Selector) -> UIBarButtonItem {
        let item = UIBarButtonItem(image: symbol == "textformat.size" ? Self.formatIcon : UIImage(systemName: symbol),style: .plain,target: self,action: selector)
        item.accessibilityLabel = labels[key] ?? key
        item.accessibilityIdentifier = "fei.edit."+id
        return item
    }
    func update(_ next: FeiNavigationState,coordinator: FeiNavigationCoordinator?,typing: Bool = false) {
        state = next; self.coordinator = coordinator
        self.typing = typing
        let phone = UIDevice.current.userInterfaceIdiom == .phone
        let palette = next.theme ?? .system
        tintColor = palette.accentColor
        let appearance = UIToolbarAppearance()
        appearance.configureWithDefaultBackground()
        if let color = palette.chromeColor { appearance.backgroundColor = color }
        standardAppearance = appearance; scrollEdgeAppearance = appearance; compactAppearance = appearance
        let nextMode = next.kind == "history-preview" ? "preview" : next.can_edit && typing ? "editing" : next.write_allowed && (next.can_edit || next.reading) ? "reading" : "locked"
        let reduced = UIApplication.shared.preferredContentSizeCategory.isAccessibilityCategory
        if mode != nextMode || labels != next.labels || composeAllowed != (next.can_create && coordinator?.hasPadWorkspace != true) || reducedTools != reduced {
            reducedTools = reduced
            composeAllowed = next.can_create && coordinator?.hasPadWorkspace != true
            mode = nextMode; labels = next.labels; commandButtons = []; tableButton = nil
            if !phone {
                navigationItems = [button("textformat.size","format-text","styles",#selector(stylesCommand)),button("checklist","todo","list",#selector(listCommand)),button("tablecells","table","table",#selector(tableCommand)),button("paperclip","insert","insert",#selector(insertCommand))]
            }
            if nextMode == "preview" {
                let marks = button("highlighter","history-marks","history-marks",#selector(historyMarks))
                let restore = button("arrow.counterclockwise","history-restore","history-restore",#selector(historyRestore))
                var list = [marks,spacer()]
                commandButtons = [marks,restore]
                if next.local_history?.can_copy == true {
                    let copy = button("doc.on.doc","history-copy","history-copy",#selector(historyCopy)); list += [copy,spacer()]; commandButtons.append(copy)
                }
                items = list+[restore]
            } else if nextMode == "editing" {
                let undo = button("arrow.uturn.backward","undo","undo",#selector(undoCommand))
                undo.menu = UIMenu(children: [UIAction(title: labels["redo"] ?? "Redo",image: UIImage(systemName: "arrow.uturn.forward")) { [weak self] _ in self?.send("redo") }])
                let list = button("checklist","todo","list",#selector(listCommand))
                let table = button("tablecells","table","table",#selector(tableCommand)); tableButton = table
                let styles = button("textformat.size","format-text","styles",#selector(stylesCommand))
                let insert = button("paperclip","insert","insert",#selector(insertCommand))
                keyboardButton.target = self; keyboardButton.action = #selector(keyboardCommand)
                keyboardButton.accessibilityIdentifier = "fei.edit.keyboard"
                commandButtons = [undo,list,styles,table,insert,keyboardButton]
                items = phone ? (reduced ? [styles,insert,spacer(),keyboardButton] : [styles,list,table,insert,spacer(),keyboardButton]) : (reduced ? [styles,insert,spacer(),undo,keyboardButton] : [styles,list,table,insert,spacer(),undo,keyboardButton])
                updateKeyboardImage()
            } else {
                let status = UILabel()
                status.font = .preferredFont(forTextStyle: .subheadline)
                status.textColor = .secondaryLabel
                status.adjustsFontForContentSizeCategory = true
                status.text = labels[nextMode == "reading" ? "reading" : "not-editable"]
                let text = UIBarButtonItem(customView: status)
                if nextMode == "reading" {
                    let edit = button("pencil","edit-note","mode",#selector(editCommand))
                    let checklist = button("checklist","todo","quick-checklist",#selector(quickChecklist))
                    let insert = button("paperclip","insert","quick-insert",#selector(quickInsert))
                    var list = reduced ? [edit,insert,spacer()] : [checklist,insert,edit,spacer()]
                    commandButtons = [checklist,insert,edit]
                    if composeAllowed {
                        let compose = button("square.and.pencil","new","compose",#selector(composeNote))
                        compose.tintColor = palette.accentColor
                        if #available(iOS 26.0,*) { compose.sharesBackground = false }
                        list.append(compose); commandButtons.append(compose)
                    }
                    items = list
                } else { items = [text,spacer()] }
            }
        }
        navigationItems.forEach { $0.isEnabled = next.can_edit && !next.busy }
        items?.filter { $0.accessibilityIdentifier == "fei.edit.compose" }.forEach { $0.tintColor = palette.accentColor }
        tableButton?.accessibilityLabel = next.labels[next.table_active ? "table-controls" : "table"]
        commandButtons.forEach { $0.isEnabled = !next.busy && ($0.accessibilityIdentifier != "fei.edit.history-restore" || next.local_history?.can_restore == true) }
        if nextMode == "preview" { commandButtons.first?.tintColor = next.local_history?.marks == true ? palette.accentColor : .secondaryLabel }
    }
    func refreshContentSize() { if let state { update(state,coordinator: coordinator,typing: typing) } }
    func updateKeyboardVisibility() {
        guard let parent = superview?.superview else { return }
        let guide = parent.keyboardLayoutGuide.layoutFrame
        let visible = guide.intersects(parent.bounds) && guide.minY < parent.safeAreaLayoutGuide.layoutFrame.maxY-1
        if keyboardVisible != visible { keyboardVisible = visible; updateKeyboardImage() }
    }
    private func updateKeyboardImage() {
        keyboardButton.image = UIImage(systemName: keyboardVisible ? "keyboard.chevron.compact.down" : "keyboard")
        keyboardButton.accessibilityLabel = labels[keyboardVisible ? "hide-keyboard" : "keyboard"]
    }
    private func send(_ command:String) {
        guard let state,state.kind == "note",!state.busy else { return }
        coordinator?.send("toolbar-command",value: command,id: state.current,scope: state.editor_scope)
    }
    @objc private func historyMarks() { if let h = state?.local_history { coordinator?.sendHistory("marks",history: h) } }
    @objc private func historyCopy() { if let h = state?.local_history { coordinator?.sendHistory("copy",name: h.selected,history: h) } }
    @objc private func historyRestore() { if let h = state?.local_history,h.can_restore { coordinator?.confirmHistory("restore",history: h) } }
    @objc private func undoCommand() { send("undo") }
    @objc private func listCommand() { send("todo") }
    @objc private func tableCommand() { send(state?.table_active == true ? "table-tools" : "table") }
    @objc private func quickChecklist() { send("quick-checklist") }
    @objc private func quickInsert() { send("quick-insert") }
    @objc private func composeNote() { guard mode == "reading" else { return }; coordinator?.composeNote() }
    @objc private func stylesCommand() { send("styles") }
    @objc private func insertCommand() { send("insert") }
    @objc private func editCommand() { send(state?.can_edit == true ? "keyboard" : "edit") }
    @objc private func keyboardCommand() {
        updateKeyboardVisibility()
        send(keyboardVisible ? "done" : "keyboard")
    }
}
