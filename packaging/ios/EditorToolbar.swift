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
    private var topTools = false
    private var showingCompose = false
    let componentBar = UIToolbar()
    private(set) var hasComponentRow = false
    var onComponentRowChanged: (() -> Void)?
    private var slashButton: UIBarButtonItem?
    private var editingAllowed = false
    private var labels: [String:String] = [:]
    private var keyboardVisible = false
    private let keyboardButton = UIBarButtonItem()
    private var tableButton: UIBarButtonItem?
    private var blockButton: UIBarButtonItem?
    private var commandButtons: [UIBarButtonItem] = []
    private var typing = false
    private var showsBlockActions = false
    private(set) var navigationItems: [UIBarButtonItem] = []
    private let phoneEditingGlass = UIVisualEffectView()
    private let phoneEditingScroll = EditingScrollView()
    private let phoneEditingStack = UIStackView()
    private var phoneEditingLeading: NSLayoutConstraint?
    private var phoneEditingTrailing: NSLayoutConstraint?
    private var phoneEditingMinimumWidth: NSLayoutConstraint?
    private var phoneEditingCommands: EditingCommands?
    private var phoneEditingButtons: [(item: UIBarButtonItem, button: UIButton)] = []

    private struct EditingCommands {
        let checklist, table, slash, block, styles, insert, undo, keyboard: UIBarButtonItem
        var components: [UIBarButtonItem] { [checklist,table,slash,block] }
        var all: [UIBarButtonItem] { components + [styles,insert,undo,keyboard] }
    }

    private final class EditingScrollView: UIScrollView {
        override func touchesShouldCancel(in view: UIView) -> Bool {
            if view is UIButton { return true }
            return super.touchesShouldCancel(in: view)
        }
    }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        accessibilityIdentifier = "fei.editor.toolbar"
        componentBar.translatesAutoresizingMaskIntoConstraints = false
        componentBar.accessibilityIdentifier = "fei.editor.components"
        componentBar.isHidden = true
        if UIDevice.current.userInterfaceIdiom == .phone { installPhoneEditingStrip() }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    private func spacer() -> UIBarButtonItem { UIBarButtonItem(barButtonSystemItem: .flexibleSpace,target: nil,action: nil) }
    private func button(_ symbol:String,_ key:String,_ id:String,_ selector:Selector) -> UIBarButtonItem {
        let item = UIBarButtonItem(image: symbol == "textformat.size" ? Self.formatIcon : UIImage(systemName: symbol),style: .plain,target: self,action: selector)
        item.accessibilityLabel = labels[key] ?? key
        item.accessibilityIdentifier = "fei.edit."+id
        return item
    }
    private func undoItem() -> UIBarButtonItem {
        let undo = UIAction { [weak self] _ in self?.send("undo") }
        let redo = UIAction(title: labels["redo"] ?? "Redo",image: UIImage(systemName: "arrow.uturn.forward")) { [weak self] _ in self?.send("redo") }
        let item = UIBarButtonItem(image: UIImage(systemName: "arrow.uturn.backward"),primaryAction: undo,menu: UIMenu(children: [redo]))
        item.accessibilityIdentifier = "fei.edit.undo"
        item.accessibilityLabel = labels["undo"]
        return item
    }
    private func editingCommands(block: UIBarButtonItem) -> EditingCommands {
        keyboardButton.target = self; keyboardButton.action = #selector(keyboardCommand)
        keyboardButton.accessibilityIdentifier = "fei.edit.keyboard"
        return EditingCommands(
            checklist: button("checklist","todo","list",#selector(listCommand)),
            table: button("tablecells","table","table",#selector(tableCommand)),
            slash: button("slash.circle","slash","slash",#selector(slashCommand)),
            block: block,
            styles: button("textformat.size","format-text","styles",#selector(stylesCommand)),
            insert: button("paperclip","insert","insert",#selector(insertCommand)),
            undo: undoItem(), keyboard: keyboardButton
        )
    }
    private func needsComponentRow(_ state: FeiNavigationState) -> Bool {
        guard UIDevice.current.userInterfaceIdiom != .phone,state.kind == "note",state.can_edit,typing else { return false }
        return bounds.width < 500 || UIApplication.shared.preferredContentSizeCategory.isAccessibilityCategory
    }
    override func layoutSubviews() {
        if let leading = phoneEditingLeading, let trailing = phoneEditingTrailing, bounds.width > 0 {
            // Match the native reading toolbar's compact and wide iPhone margins.
            let inset: CGFloat = bounds.width >= 414 ? 20 : 16
            if leading.constant != inset {
                leading.constant = inset
                trailing.constant = -inset
            }
        }
        super.layoutSubviews()
        if let state,needsComponentRow(state) != hasComponentRow { update(state,coordinator: coordinator,typing: typing,topTools: topTools) }
    }
    func update(_ next: FeiNavigationState,coordinator: FeiNavigationCoordinator?,typing: Bool = false,topTools: Bool = false) {
        state = next; self.coordinator = coordinator
        self.typing = typing
        let phone = UIDevice.current.userInterfaceIdiom == .phone
        let palette = next.theme ?? .system
        tintColor = palette.accentColor
        let appearance = UIToolbarAppearance()
        appearance.configureWithTransparentBackground()
        if #unavailable(iOS 26.0) {
            appearance.backgroundEffect = UIBlurEffect(style: .systemChromeMaterial)
            if let color = palette.chromeColor { appearance.backgroundColor = color.withAlphaComponent(0.65) }
        }
        for bar in [self,componentBar] {
            bar.tintColor = palette.accentColor
            bar.standardAppearance = appearance; bar.scrollEdgeAppearance = appearance; bar.compactAppearance = appearance
        }
        let nextMode = next.kind == "history-preview" ? "preview" : next.can_edit && typing ? "editing" : next.write_allowed && (next.can_edit || next.reading) ? "reading" : "locked"
        let stacked = needsComponentRow(next)
        let blockTools = next.can_edit && (!phone || nextMode == "editing" || next.block_selected)
        let labelsChanged = labels != next.labels
        let showCompose = phone && next.kind == "note" && next.current != nil && next.can_create && !typing
        if mode != nextMode || labelsChanged || hasComponentRow != stacked || editingAllowed != next.can_edit || showsBlockActions != blockTools ||
            self.topTools != topTools || showingCompose != showCompose {
            self.topTools = topTools
            showingCompose = showCompose
            let rowChanged = hasComponentRow != stacked
            hasComponentRow = stacked
            editingAllowed = next.can_edit
            showsBlockActions = blockTools
            mode = nextMode; labels = next.labels; commandButtons = []; tableButton = nil; slashButton = nil
            componentBar.items = nil
            let block = button("square.stack","block-actions","block-actions",#selector(blockActions))
            blockButton = block
            if phone && (phoneEditingCommands == nil || labelsChanged) {
                let commands = editingCommands(block: button("square.stack","block-actions","block-actions",#selector(blockActions)))
                phoneEditingCommands = commands
                // Have the controls laid out while reading, before the keyboard starts moving the bar.
                UIView.performWithoutAnimation {
                    updateKeyboardImage()
                    setPhoneEditingItems(commands.all)
                    if bounds.width > 0 { layoutIfNeeded() }
                }
            }
            if !phone {
                navigationItems = [button("checklist","todo","list",#selector(listCommand)),button("tablecells","table","table",#selector(tableCommand))]
                navigationItems.append(button("slash.circle","slash","slash",#selector(slashCommand)))
                navigationItems += [button("textformat.size","format-text","styles",#selector(stylesCommand)),button("paperclip","insert","insert",#selector(insertCommand))]
                if blockTools { navigationItems.append(button("square.stack","block-actions","block-actions",#selector(blockActions))) }
            }
            let compose: UIBarButtonItem? = showCompose ? makeComposeItem() : nil
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
                let editing = phoneEditingCommands ?? editingCommands(block: block)
                tableButton = editing.table; slashButton = editing.slash; blockButton = editing.block
                commandButtons = editing.all
                if phone {
                    setItems(nil,animated: false)
                } else {
                    let commands = [editing.styles,editing.insert,spacer(),editing.undo,editing.keyboard]
                    componentBar.items = stacked ? [spacer()] + editing.components + [spacer()] : nil
                    items = stacked ? commands : editing.components + commands
                }
                updateKeyboardImage()
            } else if topTools {
                setItems(nil,animated: false)
            } else {
                if nextMode == "reading" {
                    let edit = next.can_edit ? button("keyboard","keyboard","mode",#selector(editCommand))
                        : button("pencil","edit-note","mode",#selector(editCommand))
                    let checklist = button("checklist","todo","quick-checklist",#selector(quickChecklist))
                    let insert = button("paperclip","insert","quick-insert",#selector(quickInsert))
                    var list: [UIBarButtonItem] = []
                    if let compose { list += [compose,spacer()] }
                    list += [checklist,insert]
                    if blockTools { list.append(block) }
                    list.append(spacer())
                    commandButtons = [checklist,insert,edit]
                    if phone && next.can_edit {
                        let undo = undoItem()
                        list.append(undo); commandButtons.append(undo)
                    }
                    list.append(edit)
                    if let compose { commandButtons.append(compose) }
                    items = list
                } else {
                    let status = UILabel()
                    status.font = .preferredFont(forTextStyle: .subheadline)
                    status.textColor = .secondaryLabel
                    status.adjustsFontForContentSizeCategory = true
                    status.adjustsFontSizeToFitWidth = true
                    status.minimumScaleFactor = 0.65
                    status.textAlignment = .center
                    status.text = labels["not-editable"]
                    status.accessibilityIdentifier = "fei.editor.read-only"
                    let message = [spacer(),UIBarButtonItem(customView: status),spacer()]
                    if let compose {
                        let balance = UIBarButtonItem(barButtonSystemItem: .fixedSpace,target: nil,action: nil)
                        balance.width = compose.width
                        items = [compose]+message+[balance]
                        commandButtons.append(compose)
                    } else { items = message }
                }
            }
            if rowChanged { onComponentRowChanged?() }
        }
        navigationItems.forEach {
            $0.isEnabled = next.can_edit && !next.busy && ($0.accessibilityIdentifier != "fei.edit.slash" || next.slash_available)
        }
        tableButton?.accessibilityLabel = next.labels[next.table_active ? "table-controls" : "table"]
        commandButtons.forEach { $0.isEnabled = !next.busy && ($0.accessibilityIdentifier != "fei.edit.history-restore" || next.local_history?.can_restore == true) }
        slashButton?.isEnabled = next.slash_available && !next.busy
        blockButton?.isEnabled = next.block_actions && next.can_edit && !next.busy &&
            (phone || typing || next.block_selected)
        if nextMode == "preview" { commandButtons.first?.tintColor = next.local_history?.marks == true ? palette.accentColor : .secondaryLabel }
        if phone { updatePhoneEditingStrip(visible: nextMode == "editing") }
    }
    private func makeComposeItem() -> UIBarButtonItem {
        let action = UIAction(title: labels["new"] ?? "") { [weak self] _ in
            self?.coordinator?.composeFromEditor()
        }
        let item = UIBarButtonItem(systemItem: .compose,primaryAction: action)
        item.width = 44
        item.accessibilityLabel = action.title
        item.accessibilityIdentifier = "fei.new.editor"
        return item
    }

    private func installPhoneEditingStrip() {
        let glass = phoneEditingGlass
        let scroll = phoneEditingScroll
        let stack = phoneEditingStack
        if #available(iOS 26.0,*) {
            let effect = UIGlassEffect()
            effect.isInteractive = true
            glass.effect = effect
            glass.cornerConfiguration = .capsule()
            glass.contentView.tintColor = .label
        }
        glass.isHidden = true
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.delaysContentTouches = false
        scroll.isDirectionalLockEnabled = true
        scroll.showsVerticalScrollIndicator = false
        scroll.accessibilityIdentifier = "fei.editor.editing-actions"
        stack.distribution = .fillEqually
        for view in [glass,scroll,stack] { view.translatesAutoresizingMaskIntoConstraints = false }
        addSubview(glass)
        glass.contentView.addSubview(scroll)
        scroll.addSubview(stack)

        let leading = glass.leadingAnchor.constraint(equalTo: leadingAnchor,constant: 16)
        let trailing = glass.trailingAnchor.constraint(equalTo: trailingAnchor,constant: -16)
        phoneEditingLeading = leading; phoneEditingTrailing = trailing
        let minimum = stack.widthAnchor.constraint(greaterThanOrEqualToConstant: 0)
        phoneEditingMinimumWidth = minimum
        let fits = stack.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor,constant: -12)
        fits.priority = .defaultHigh
        NSLayoutConstraint.activate([
            leading, trailing,
            glass.centerYAnchor.constraint(equalTo: centerYAnchor),
            glass.heightAnchor.constraint(equalToConstant: 48),
            scroll.leadingAnchor.constraint(equalTo: glass.contentView.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: glass.contentView.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: glass.contentView.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: glass.contentView.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor,constant: 6),
            stack.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor,constant: -6),
            stack.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
            stack.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
            stack.heightAnchor.constraint(equalTo: scroll.frameLayoutGuide.heightAnchor), minimum, fits
        ])
    }
    private func updatePhoneEditingStrip(visible: Bool) {
        guard phoneEditingGlass.isHidden == visible else {
            if visible { refreshPhoneEditingItems() }
            return
        }
        // The keyboard guide animates the toolbar itself. Commit local geometry without
        // interpolating the glass, scroll offset or button layout inside that animation.
        UIView.performWithoutAnimation {
            if visible {
                refreshPhoneEditingItems()
                if bounds.width > 0 { layoutIfNeeded() }
                phoneEditingScroll.setContentOffset(.zero,animated: false)
            }
            phoneEditingGlass.isHidden = !visible
        }
    }
    private func setPhoneEditingItems(_ items: [UIBarButtonItem]) {
        phoneEditingStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        phoneEditingButtons = items.map { item in
            let control = UIButton(type: .system)
            control.accessibilityIdentifier = item.accessibilityIdentifier
            control.menu = item.menu
            control.setImage(item.image,for: .normal)
            if let action = item.primaryAction { control.addAction(action,for: .primaryActionTriggered) }
            else if let action = item.action { control.addTarget(item.target,action: action,for: .primaryActionTriggered) }
            phoneEditingStack.addArrangedSubview(control)
            return (item,control)
        }
        updatePhoneEditingSize()
    }
    private func updatePhoneEditingSize() {
        guard !phoneEditingButtons.isEmpty else { return }
        let metrics = UIFontMetrics(forTextStyle: .body)
        let width = min(56,max(44,metrics.scaledValue(for: 44,compatibleWith: traitCollection)))
        let pointSize = min(26,max(21,metrics.scaledValue(for: 21,compatibleWith: traitCollection)))
        phoneEditingMinimumWidth?.constant = width * CGFloat(phoneEditingButtons.count)
        let symbols = UIImage.SymbolConfiguration(pointSize: pointSize,weight: .regular)
        phoneEditingButtons.forEach { $0.button.setPreferredSymbolConfiguration(symbols,forImageIn: .normal) }
    }
    private func refreshPhoneEditingItems() {
        for (item,control) in phoneEditingButtons {
            control.accessibilityLabel = item.accessibilityLabel
            control.isEnabled = item.isEnabled
            if control.image(for: .normal) != item.image { control.setImage(item.image,for: .normal) }
        }
    }
    func refreshContentSize() {
        if let state { update(state,coordinator: coordinator,typing: typing) }
        updatePhoneEditingSize()
    }
    func updateKeyboardVisibility() {
        guard let parent = superview?.superview else { return }
        let guide = parent.keyboardLayoutGuide.layoutFrame
        let visible = guide.intersects(parent.bounds) && guide.minY < parent.safeAreaLayoutGuide.layoutFrame.maxY-1
        if keyboardVisible != visible { keyboardVisible = visible; updateKeyboardImage() }
    }
    private func updateKeyboardImage() {
        keyboardButton.image = UIImage(systemName: keyboardVisible ? "keyboard.chevron.compact.down" : "keyboard")
        keyboardButton.accessibilityLabel = labels[keyboardVisible ? "hide-keyboard" : "keyboard"]
        if !phoneEditingGlass.isHidden { refreshPhoneEditingItems() }
    }
    private func send(_ command:String) {
        guard let state,state.kind == "note",!state.busy else { return }
        coordinator?.send("toolbar-command",value: command,id: state.current,scope: state.editor_scope)
    }
    @objc private func historyMarks() { if let h = state?.local_history { coordinator?.sendHistory("marks",history: h) } }
    @objc private func historyCopy() { if let h = state?.local_history { coordinator?.sendHistory("copy",name: h.selected,history: h) } }
    @objc private func historyRestore() { if let h = state?.local_history,h.can_restore { coordinator?.confirmHistory("restore",history: h) } }
    @objc private func slashCommand() { send("slash") }
    @objc private func listCommand() { send("todo") }
    @objc private func tableCommand() { send(state?.table_active == true ? "table-tools" : "table") }
    @objc private func quickChecklist() { send("quick-checklist") }
    @objc private func quickInsert() { send("quick-insert") }
    @objc private func stylesCommand() { send("styles") }
    @objc private func blockActions() { send("block-actions") }
    @objc private func insertCommand() { send("insert") }
    @objc private func editCommand() { send(state?.can_edit == true ? "keyboard" : "edit") }
    @objc private func keyboardCommand() {
        updateKeyboardVisibility()
        send(keyboardVisible ? "done" : "keyboard")
    }
}
