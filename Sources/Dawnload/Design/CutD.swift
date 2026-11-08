import SwiftUI

/// The Cut D, from the same geometry as docs/brand/source/geo.py.
/// Units: the half-disc has radius s; y points down; the D is (SPINE + 1)·s wide and 2·s tall.
struct CutDGeometry {
    static let spine: CGFloat = 0.53
    static let corner: CGFloat = 0.18
    static let width: CGFloat = spine + 1
    static let arrowTop: CGFloat = 0.48
    static let arrowTip: CGFloat = 1.40
    static let arrowHead: CGFloat = 0.44
    static let arrowWeight: CGFloat = 0.32
    static let cutWeight: CGFloat = 0.34
    static let dotsWeight: CGFloat = 0.30
    /// Width ÷ height of the D.
    static let aspect: CGFloat = width / 2

    let x: CGFloat
    let y: CGFloat
    let s: CGFloat

    /// A D centred in `size`, as large as fits.
    init(fitting size: CGSize) {
        let s = min(size.height / 2, size.width / Self.width)
        self.s = s
        x = (size.width - Self.width * s) / 2
        y = (size.height - 2 * s) / 2
    }

    init(x: CGFloat, y: CGFloat, s: CGFloat) {
        self.x = x
        self.y = y
        self.s = s
    }

    var center: CGPoint { CGPoint(x: x + Self.width * s / 2, y: y + s) }
    var bounds: CGRect { CGRect(x: x, y: y, width: Self.width * s, height: 2 * s) }

    /// The solid D.
    var dShape: Path { Self.d(x: x, y: y, s: s) }

    /// The inner D the Queued outline is cut with.
    var queuedHole: Path { Self.d(x: x + 0.24 * s, y: y + 0.24 * s, s: s * 0.76) }

    static func d(x: CGFloat, y: CGFloat, s: CGFloat) -> Path {
        var p = Path()
        let sp = spine * s
        let c = corner * s
        let k: CGFloat = 0.5522847498
        p.move(to: CGPoint(x: x + sp, y: y))
        arc(&p, cx: x + sp, cy: y + s, r: s, from: -90, to: 90)
        p.addLine(to: CGPoint(x: x + c, y: y + 2 * s))
        p.addCurve(to: CGPoint(x: x, y: y + 2 * s - c),
                   control1: CGPoint(x: x + c - k * c, y: y + 2 * s),
                   control2: CGPoint(x: x, y: y + 2 * s - c + k * c))
        p.addLine(to: CGPoint(x: x, y: y + c))
        p.addCurve(to: CGPoint(x: x + c, y: y),
                   control1: CGPoint(x: x, y: y + c - k * c),
                   control2: CGPoint(x: x + c - k * c, y: y))
        p.closeSubpath()
        return p
    }

    /// Cubic arc, y down, angles in degrees, clockwise positive (as geo.py's _arc).
    private static func arc(_ p: inout Path, cx: CGFloat, cy: CGFloat, r: CGFloat, from a0: CGFloat, to a1: CGFloat) {
        let total = a1 - a0
        let n = max(1, Int((abs(total) / 90).rounded(.up)))
        let step = total / CGFloat(n) * .pi / 180
        var a = a0 * .pi / 180
        let h = 4.0 / 3.0 * tan(step / 4)
        for _ in 0..<n {
            let b = a + step
            let (x0, y0) = (cos(a), sin(a))
            let (x1, y1) = (cos(b), sin(b))
            p.addCurve(
                to: CGPoint(x: cx + r * x1, y: cy + r * y1),
                control1: CGPoint(x: cx + r * (x0 - h * y0), y: cy + r * (y0 + h * x0)),
                control2: CGPoint(x: cx + r * (x1 + h * y1), y: cy + r * (y1 - h * x1))
            )
            a = b
        }
    }

    /// A cut is a set of round-capped strokes and dots punched out of the D.
    struct Cut {
        var strokes: [Path] = []
        var weight: CGFloat
        var dots: [Path] = []
    }

    func cut(_ kind: CutDGlyph.Cut, arrowOffset: CGFloat = 0) -> Cut {
        let (cx, cy) = (center.x, center.y)
        switch kind {
        case .arrow, .arrowUp:
            let w = Self.arrowWeight * s
            let top = y + Self.arrowTop * s + arrowOffset
            let tip = y + Self.arrowTip * s + arrowOffset
            let head = Self.arrowHead * s
            var shaft = Path()
            var wings = Path()
            if kind == .arrowUp {
                // The same arrow turned to point up, mirrored about the D's middle.
                let flip = { (v: CGFloat) in 2 * cy - v }
                shaft.move(to: CGPoint(x: cx, y: flip(top)))
                shaft.addLine(to: CGPoint(x: cx, y: flip(tip)))
                wings.move(to: CGPoint(x: cx - head, y: flip(tip - head)))
                wings.addLine(to: CGPoint(x: cx, y: flip(tip)))
                wings.addLine(to: CGPoint(x: cx + head, y: flip(tip - head)))
            } else {
                shaft.move(to: CGPoint(x: cx, y: top))
                shaft.addLine(to: CGPoint(x: cx, y: tip))
                wings.move(to: CGPoint(x: cx - head, y: tip - head))
                wings.addLine(to: CGPoint(x: cx, y: tip))
                wings.addLine(to: CGPoint(x: cx + head, y: tip - head))
            }
            return Cut(strokes: [shaft, wings], weight: w)

        case .check:
            var p = Path()
            p.move(to: CGPoint(x: cx - 0.40 * s, y: cy + 0.02 * s))
            p.addLine(to: CGPoint(x: cx - 0.10 * s, y: cy + 0.32 * s))
            p.addLine(to: CGPoint(x: cx + 0.42 * s, y: cy - 0.26 * s))
            return Cut(strokes: [p], weight: Self.cutWeight * s)

        case .bang:
            let w = Self.cutWeight * s
            var p = Path()
            p.move(to: CGPoint(x: cx, y: cy - 0.48 * s))
            p.addLine(to: CGPoint(x: cx, y: cy + 0.08 * s))
            let r = w * 0.62
            let dot = Path(ellipseIn: CGRect(x: cx - r, y: cy + 0.44 * s - r, width: 2 * r, height: 2 * r))
            return Cut(strokes: [p], weight: w, dots: [dot])

        case .pause:
            let w = Self.cutWeight * s * 0.9
            let g = 0.20 * s
            var left = Path()
            left.move(to: CGPoint(x: cx - g, y: cy - 0.36 * s))
            left.addLine(to: CGPoint(x: cx - g, y: cy + 0.36 * s))
            var right = Path()
            right.move(to: CGPoint(x: cx + g, y: cy - 0.36 * s))
            right.addLine(to: CGPoint(x: cx + g, y: cy + 0.36 * s))
            return Cut(strokes: [left, right], weight: w)

        case .merge:
            let w = Self.cutWeight * s * 0.9
            var v = Path()
            v.move(to: CGPoint(x: cx - 0.36 * s, y: cy - 0.46 * s))
            v.addLine(to: CGPoint(x: cx, y: cy - 0.06 * s))
            v.addLine(to: CGPoint(x: cx + 0.36 * s, y: cy - 0.46 * s))
            var stem = Path()
            stem.move(to: CGPoint(x: cx, y: cy - 0.06 * s))
            stem.addLine(to: CGPoint(x: cx, y: cy + 0.48 * s))
            return Cut(strokes: [v, stem], weight: w)

        case .dots:
            let r = Self.dotsWeight * s * 0.55
            let dots = [-0.34, 0, 0.34].map { dx in
                Path(ellipseIn: CGRect(x: cx + dx * s - r, y: cy - r, width: 2 * r, height: 2 * r))
            }
            return Cut(weight: 0, dots: dots)
        }
    }
}

/// A status glyph: the D with a state's cut, filled to a fraction from the top.
struct CutDGlyph: View, Animatable {
    enum Cut: Equatable {
        case arrow, arrowUp, check, bang, pause, merge, dots
    }

    enum Style: Equatable {
        /// Solid D in one colour with the cut punched out.
        case solid
        /// The D drawn in `track`, filled from the top to `fill` in the colour.
        case filling
        /// Queued: the D as an outline with dots.
        case outline
        /// The arrow travelling down through the D, on repeat.
        case loader
    }

    var cut: Cut
    var style: Style
    var color: Color
    var track: Color = DLColor.track
    var fill: Double = 1

    var animatableData: Double {
        get { fill }
        set { fill = newValue }
    }

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if style == .loader && !reduceMotion {
            TimelineView(.animation) { timeline in
                let t = timeline.date.timeIntervalSinceReferenceDate
                let phase = (t.truncatingRemainder(dividingBy: DLMotion.loaderPeriod)) / DLMotion.loaderPeriod
                canvas(loaderPhase: ease(phase))
            }
        } else {
            canvas(loaderPhase: nil)
        }
    }

    private func canvas(loaderPhase: Double?) -> some View {
        Canvas { context, size in
            let geo = CutDGeometry(fitting: size)
            switch style {
            case .outline:
                context.drawLayer { layer in
                    layer.fill(geo.dShape, with: .color(color))
                    layer.blendMode = .destinationOut
                    layer.fill(geo.queuedHole, with: .color(.black))
                }
                for dot in geo.cut(.dots).dots {
                    context.fill(dot, with: .color(color))
                }

            case .solid:
                drawCutD(in: &context, geo: geo, color: color, cut: geo.cut(cut))

            case .loader:
                if let loaderPhase {
                    let travel = 1.6 * geo.s
                    let offset = -travel + 2 * travel * loaderPhase
                    context.drawLayer { layer in
                        layer.fill(geo.dShape, with: .color(color))
                        layer.blendMode = .destinationOut
                        for extra in [offset, offset - travel] {
                            punch(geo.cut(.arrow, arrowOffset: extra), in: &layer)
                        }
                    }
                } else {
                    drawCutD(in: &context, geo: geo, color: color, cut: geo.cut(.arrow))
                }

            case .filling:
                drawCutD(in: &context, geo: geo, color: track, cut: geo.cut(cut))
                let clamped = min(max(fill, 0), 1)
                if clamped > 0 {
                    let bounds = geo.bounds
                    let visible = CGRect(x: 0, y: 0, width: size.width, height: bounds.minY + bounds.height * clamped)
                    context.drawLayer { layer in
                        layer.clip(to: Path(visible))
                        drawCutD(in: &layer, geo: geo, color: color, cut: geo.cut(cut))
                    }
                }
            }
        }
        .aspectRatio(CutDGeometry.aspect, contentMode: .fit)
    }

    private func drawCutD(in context: inout GraphicsContext, geo: CutDGeometry, color: Color, cut: CutDGeometry.Cut) {
        context.drawLayer { layer in
            layer.fill(geo.dShape, with: .color(color))
            layer.blendMode = .destinationOut
            punch(cut, in: &layer)
        }
    }

    private func punch(_ cut: CutDGeometry.Cut, in layer: inout GraphicsContext) {
        let style = StrokeStyle(lineWidth: cut.weight, lineCap: .round, lineJoin: .round)
        for stroke in cut.strokes {
            layer.stroke(stroke, with: .color(.black), style: style)
        }
        for dot in cut.dots {
            layer.fill(dot, with: .color(.black))
        }
    }

    /// keySplines 0.55 0 0.45 1, as the brand loader.
    private func ease(_ t: Double) -> Double {
        cubicBezier(t, x1: 0.55, y1: 0, x2: 0.45, y2: 1)
    }
}

/// CSS-style cubic-bezier timing: solves x(u) = t, returns y(u).
func cubicBezier(_ t: Double, x1: Double, y1: Double, x2: Double, y2: Double) -> Double {
    func coordinate(_ u: Double, _ p1: Double, _ p2: Double) -> Double {
        let v = 1 - u
        return 3 * v * v * u * p1 + 3 * v * u * u * p2 + u * u * u
    }
    var lo = 0.0
    var hi = 1.0
    var u = t
    for _ in 0..<24 {
        let x = coordinate(u, x1, x2)
        if abs(x - t) < 1e-5 { break }
        if x < t { lo = u } else { hi = u }
        u = (lo + hi) / 2
    }
    return coordinate(u, y1, y2)
}

/// The mark: the D with the arrow, in Signal.
struct DawnloadMark: View {
    var color: Color = DLColor.progress

    var body: some View {
        CutDGlyph(cut: .arrow, style: .solid, color: color)
            .accessibilityHidden(true)
    }
}
