import UIKit

// UIKit supplies gestures; the shared editor computes and commits the crop.
@MainActor
final class FeiImageCrop: UIView {
    private let imageView = UIImageView()
    private let maskLayer = CAShapeLayer()
    private let borderLayer = CAShapeLayer()
    private let hint = UILabel()
    private var handles: [(UIView,Int,Int)] = []
    private var rect: [CGFloat] = [0,0,1,1]
    private var path: String?
    private let send: (String,String) -> Void
    private var imageRect: CGRect {
        guard let size = imageView.image?.size,size.width > 0,size.height > 0 else { return .zero }
        let area = bounds.insetBy(dx: 22,dy: 38)
        let scale = min(area.width/size.width,area.height/size.height)
        return CGRect(x: area.midX-size.width*scale/2,y: area.midY-size.height*scale/2,width: size.width*scale,height: size.height*scale)
    }
    init(send: @escaping (String,String) -> Void) {
        self.send = send
        super.init(frame: .zero)
        accessibilityIdentifier = "fei.image.crop"
        imageView.contentMode = .scaleAspectFit
        addSubview(imageView)
        maskLayer.fillColor = UIColor.black.withAlphaComponent(0.45).cgColor
        maskLayer.fillRule = .evenOdd
        layer.addSublayer(maskLayer)
        borderLayer.fillColor = UIColor.clear.cgColor
        borderLayer.strokeColor = UIColor.white.cgColor
        borderLayer.lineWidth = 2
        layer.addSublayer(borderLayer)
        hint.font = .preferredFont(forTextStyle: .footnote)
        hint.textColor = .secondaryLabel
        hint.textAlignment = .center
        hint.adjustsFontForContentSizeCategory = true
        hint.numberOfLines = 2
        addSubview(hint)
        for (x,y) in [(0,0),(-1,-1),(1,-1),(-1,1),(1,1),(-1,0),(1,0),(0,-1),(0,1)] {
            let view = UIView()
            view.accessibilityIdentifier = x == 0 && y == 0 ? "fei.image.crop.move" : "fei.image.crop.\(x).\(y)"
            view.isAccessibilityElement = true
            view.accessibilityTraits = .adjustable
            if x != 0 || y != 0 {
                let dot = UIView(frame: CGRect(x: 16,y: 16,width: 12,height: 12))
                dot.layer.cornerRadius = 6
                dot.backgroundColor = .white
                dot.layer.borderColor = UIColor.tintColor.cgColor
                dot.layer.borderWidth = 1
                view.addSubview(dot)
            }
            view.addGestureRecognizer(UIPanGestureRecognizer(target: self,action: #selector(pan)))
            addSubview(view)
            handles.append((view,x,y))
        }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    func update(_ field: FeiObjectForm.Request.Field) {
        if path != field.path { path = field.path; imageView.image = field.path.flatMap(UIImage.init(contentsOfFile:)) }
        if let next = field.rect,next.count == 4 { rect = next }
        hint.text = field.title
        setNeedsLayout()
    }
    override func layoutSubviews() {
        super.layoutSubviews()
        let area = imageRect
        imageView.frame = area
        hint.frame = CGRect(x: 0,y: 0,width: bounds.width,height: 36)
        let crop = CGRect(x: area.minX+rect[0]*area.width,y: area.minY+rect[1]*area.height,width: (rect[2]-rect[0])*area.width,height: (rect[3]-rect[1])*area.height)
        let mask = UIBezierPath(rect: area)
        mask.append(UIBezierPath(rect: crop))
        CATransaction.begin(); CATransaction.setDisableActions(true)
        maskLayer.path = mask.cgPath
        borderLayer.path = UIBezierPath(rect: crop).cgPath
        CATransaction.commit()
        for (view,x,y) in handles {
            if x == 0 && y == 0 { view.frame = crop.insetBy(dx: min(22,crop.width/3),dy: min(22,crop.height/3)) }
            else {
                let point = CGPoint(x: x < 0 ? crop.minX : x > 0 ? crop.maxX : crop.midX,y: y < 0 ? crop.minY : y > 0 ? crop.maxY : crop.midY)
                view.frame = CGRect(x: point.x-22,y: point.y-22,width: 44,height: 44)
            }
        }
    }
    @objc private func pan(_ gesture: UIPanGestureRecognizer) {
        guard let target = gesture.view,let (_,hx,hy) = handles.first(where: { $0.0 === target }) else { return }
        let area = imageRect
        guard area.width > 0,area.height > 0 else { return }
        let point = gesture.location(in: self)
        var value: [String: Any] = ["x":(point.x-area.minX)/area.width,"y":(point.y-area.minY)/area.height]
        let action: String
        switch gesture.state {
        case .began: action = "crop-start"; value["hx"] = hx; value["hy"] = hy
        case .changed: action = "crop-move"
        case .ended,.cancelled,.failed: send("crop-end",""); return
        default: return
        }
        if let data = try? JSONSerialization.data(withJSONObject: value),let json = String(data: data,encoding: .utf8) { send(action,json) }
    }
}
