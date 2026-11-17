import UIKit

struct FeiMobileTheme: Decodable, Equatable, Sendable {
    let id: String
    let accent: [CGFloat]
    let paper: [CGFloat]
    let folder: [CGFloat]
    let chrome: [CGFloat]?
    let list: [CGFloat]?
    let text: [CGFloat]?
    let row: [CGFloat]?
    let selected: [CGFloat]?
    let selected_text: [CGFloat]?

    static let system = FeiMobileTheme(id: "",accent: [],paper: [],folder: [],chrome: nil,list: nil,text: nil,row: nil,selected: nil,selected_text: nil)
    @MainActor private func color(_ components: [CGFloat]?) -> UIColor? {
        guard let c = components,c.count == 4,c.allSatisfy({ $0.isFinite && (0...1).contains($0) }) else { return nil }
        return UIColor(red: c[0],green: c[1],blue: c[2],alpha: c[3])
    }
    @MainActor var accentColor: UIColor { color(accent) ?? .systemBlue }
    @MainActor var paperColor: UIColor { color(paper) ?? .systemBackground }
    @MainActor var folderColor: UIColor { color(folder) ?? accentColor }
    @MainActor var chromeColor: UIColor? { color(chrome) }
    @MainActor var listColor: UIColor { color(list) ?? .systemGroupedBackground }
    @MainActor var textColor: UIColor { color(text) ?? .label }
    @MainActor var rowColor: UIColor { color(row) ?? .secondarySystemGroupedBackground }
    @MainActor var selectedColor: UIColor { color(selected) ?? .tertiarySystemFill }
    @MainActor var selectedTextColor: UIColor { color(selected_text) ?? textColor }
}

@_cdecl("fei_ios_type_scale")
@MainActor
func feiIOSTypeScale() -> Float {
    let traits = UITraitCollection(preferredContentSizeCategory: UIApplication.shared.preferredContentSizeCategory)
    return Float(UIFont.preferredFont(forTextStyle: .body,compatibleWith: traits).pointSize / 17)
}

@MainActor
func feiSharedDocumentsIcon() -> UIImage {
    UIGraphicsImageRenderer(size: CGSize(width: 28,height: 28)).image { _ in
        UIColor.systemCyan.setFill(); UIBezierPath(roundedRect: CGRect(x: 0,y: 0,width: 28,height: 28),cornerRadius: 7).fill()
        UIImage(systemName: "person.2.fill",withConfiguration: UIImage.SymbolConfiguration(pointSize: 17,weight: .medium))?.withTintColor(.white,renderingMode: .alwaysOriginal).draw(in: CGRect(x: 4,y: 6,width: 20,height: 16))
    }.withRenderingMode(.alwaysOriginal)
}
