import UIKit

/// Owns touch motion for the document. GPUI supplies a virtualized, viewport-sized
/// canvas and publishes revised bounds as more blocks have been measured.
final class FeiDocumentScrollView: UIScrollView, UIScrollViewDelegate {
    private let canvas: UIView
    private var route: String?
    private var extent: CGFloat = 0
    private var logicalOffset: CGFloat = 0
    private var needsScrollFrame = false
    private var synchronizing = false
    private var returningToTop = false
    private var awaitingLayout = false

    init(canvas: UIView) {
        self.canvas = canvas
        super.init(frame: .zero)
        contentInsetAdjustmentBehavior = .never
        showsHorizontalScrollIndicator = false
        alwaysBounceVertical = true
        isDirectionalLockEnabled = true
        delaysContentTouches = false
        canCancelContentTouches = true
        isScrollEnabled = false
        scrollsToTop = false
        delegate = self
        // The hidden UIKit text-input view is also a UIScrollView.
        // Only the document scroller should receive the status-bar gesture.
        Self.disableScrollToTop(in: canvas)
        addSubview(canvas)
    }

    private static func disableScrollToTop(in view: UIView) {
        (view as? UIScrollView)?.scrollsToTop = false
        view.subviews.forEach { disableScrollToTop(in: $0) }
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    var scrollInProgress: Bool {
        isTracking || isDragging || isDecelerating || returningToTop || needsScrollFrame
            || abs(contentOffset.y - clamped(contentOffset.y)) > 0.25
    }

    func setRoute(_ next: String?) {
        guard route != next else { return }
        route = next
        awaitingLayout = next != nil
        fei_ios_enable_native_scroll(next != nil)
        synchronizing = true
        isScrollEnabled = false
        scrollsToTop = false
        returningToTop = false
        needsScrollFrame = false
        logicalOffset = 0
        extent = 0
        contentSize = bounds.size
        setContentOffset(.zero, animated: false)
        layoutCanvas()
        synchronizing = false
    }

    private func clamped(_ offset: CGFloat) -> CGFloat { min(max(0,offset),extent) }

    func layoutCanvas() {
        // GPUI draws the document at the current viewport offset. UIKit
        // translates that viewport only for the rubber band at either end.
        if canvas.bounds.size != bounds.size {
            canvas.bounds.size = bounds.size
        }
        let center = CGPoint(x: contentOffset.x + bounds.width/2,y: clamped(contentOffset.y) + bounds.height/2)
        if canvas.center != center { canvas.center = center }
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard !synchronizing, route != nil, isScrollEnabled else { return }
        layoutCanvas()
        needsScrollFrame = true
    }

    func applyScrollForFrame() {
        if awaitingLayout {
            awaitingLayout = false
            fei_ios_scroll_editor(0,false)
        }
        guard isScrollEnabled, scrollInProgress else { return }
        needsScrollFrame = false
        // Sample UIKit immediately before GPUI draws. The delegate only wakes
        // the frame driver, so intervening layout cannot leave a stale sample.
        let offset = clamped(contentOffset.y)
        let delta = offset-logicalOffset
        guard delta != 0 else { return }
        logicalOffset = offset
        fei_ios_scroll_editor(Float(delta),offset <= 0)
    }

    func synchronizeLayout() {
        guard route != nil else { return }
        var metrics: (Float,Float) = (0,0)
        let available = withUnsafeMutableBytes(of: &metrics) { raw in
            fei_ios_native_scroll_metrics(raw.bindMemory(to: Float.self).baseAddress)
        }
        guard available else {
            if isScrollEnabled {
                synchronizing = true
                isScrollEnabled = false
                scrollsToTop = false
                returningToTop = false
                needsScrollFrame = false
                synchronizing = false
            }
            return
        }
        guard metrics.0.isFinite, metrics.1.isFinite else { return }
        #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
        gpuiScrollOffset = Double(metrics.0)
        #endif
        let moving = scrollInProgress
        synchronizing = true
        if !isScrollEnabled {
            isScrollEnabled = true
            scrollsToTop = true
        }
        let measuredExtent = CGFloat(max(metrics.1,0))
        if !moving || (measuredExtent > extent + 1 && contentOffset.y > extent - bounds.height/2) {
            extent = measuredExtent
        }
        let size = CGSize(width: bounds.width,height: bounds.height+extent)
        if abs(contentSize.height-size.height) > 1 || contentSize.width != size.width {
            contentSize = size
        }
        // Rewriting a UIKit offset while its pan is active can cancel
        // tracking, even when the destination equals the current offset.
        if !moving {
            logicalOffset = clamped(CGFloat(metrics.0))
            if abs(contentOffset.y-logicalOffset) > 0.25 {
                setContentOffset(CGPoint(x: 0,y: logicalOffset),animated: false)
            }
        }
        layoutCanvas()
        synchronizing = false
    }

    #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
    private var gpuiScrollOffset = 0.0

    var diagnosticScrollOffsets: (native: Double, forwarded: Double, gpui: Double)? {
        guard scrollInProgress else { return nil }
        return (Double(contentOffset.y), Double(logicalOffset), gpuiScrollOffset)
    }
    #endif

    func returnToTop() {
        guard isScrollEnabled, contentOffset.y > 0.25 else { return }
        returningToTop = !UIAccessibility.isReduceMotionEnabled
        setContentOffset(.zero,animated:returningToTop)
    }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) { returningToTop = false }

    func scrollViewShouldScrollToTop(_ scrollView: UIScrollView) -> Bool {
        guard isScrollEnabled, contentOffset.y > 0.25 else { return false }
        returningToTop = true
        return true
    }

    func scrollViewDidScrollToTop(_ scrollView: UIScrollView) { returningToTop = false }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        returningToTop = false
        FeiTextActions.shared.dismiss()
    }

    override func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer === panGestureRecognizer else { return super.gestureRecognizerShouldBegin(gestureRecognizer) }
        guard !fei_ios_native_scroll_reserved() else { return false }
        let velocity = panGestureRecognizer.velocity(in: self)
        if velocity == .zero {
            // Let UIKit begin a pan before the finger has picked an axis.
            // A new touch inside a horizontal block still waits for direction.
            let location = panGestureRecognizer.location(in: canvas)
            let canBegin = isDecelerating || !fei_ios_horizontal_scroll_at(Float(location.x), Float(location.y))
            return canBegin && super.gestureRecognizerShouldBegin(gestureRecognizer)
        }
        return abs(velocity.y) > abs(velocity.x) && super.gestureRecognizerShouldBegin(gestureRecognizer)
    }

    override func touchesShouldCancel(in view: UIView) -> Bool {
        !fei_ios_native_scroll_reserved() && super.touchesShouldCancel(in: view)
    }
}
