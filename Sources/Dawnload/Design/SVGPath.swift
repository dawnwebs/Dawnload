import SwiftUI

/// A small SVG path-data reader (M L H V C S Q T Z, absolute and relative) for the brand's
/// outlined artwork. Arcs aren't used by the brand files, so they aren't supported.
enum SVGPath {
    static func parse(_ data: String) -> Path {
        var path = Path()
        var scanner = Tokens(data)
        var command: Character = "M"
        var current = CGPoint.zero
        var start = CGPoint.zero
        var lastControl: CGPoint?
        var lastCommand: Character = "M"

        while let token = scanner.next() {
            switch token {
            case .command(let c):
                command = c
                if c == "Z" || c == "z" {
                    path.closeSubpath()
                    current = start
                    lastControl = nil
                    lastCommand = c
                }
                continue
            case .number(let first):
                scanner.pushBack(first)
            }

            let relative = command.isLowercase
            func point() -> CGPoint? {
                guard let x = scanner.number(), let y = scanner.number() else { return nil }
                return relative ? CGPoint(x: current.x + x, y: current.y + y) : CGPoint(x: x, y: y)
            }

            switch command.uppercased().first! {
            case "M":
                guard let p = point() else { return path }
                path.move(to: p)
                current = p
                start = p
                lastControl = nil
                // Further pairs after a moveto are linetos.
                command = relative ? "l" : "L"
            case "L":
                guard let p = point() else { return path }
                path.addLine(to: p)
                current = p
                lastControl = nil
            case "H":
                guard let x = scanner.number() else { return path }
                current = CGPoint(x: relative ? current.x + x : x, y: current.y)
                path.addLine(to: current)
                lastControl = nil
            case "V":
                guard let y = scanner.number() else { return path }
                current = CGPoint(x: current.x, y: relative ? current.y + y : y)
                path.addLine(to: current)
                lastControl = nil
            case "C":
                guard let c1 = point(), let c2 = point(), let p = point() else { return path }
                path.addCurve(to: p, control1: c1, control2: c2)
                lastControl = c2
                current = p
            case "S":
                let reflectable = "CcSs".contains(lastCommand)
                let c1 = reflectable ? reflect(lastControl, about: current) : current
                guard let c2 = point(), let p = point() else { return path }
                path.addCurve(to: p, control1: c1, control2: c2)
                lastControl = c2
                current = p
            case "Q":
                guard let c = point(), let p = point() else { return path }
                path.addQuadCurve(to: p, control: c)
                lastControl = c
                current = p
            case "T":
                let reflectable = "QqTt".contains(lastCommand)
                let c = reflectable ? reflect(lastControl, about: current) : current
                guard let p = point() else { return path }
                path.addQuadCurve(to: p, control: c)
                lastControl = c
                current = p
            default:
                return path
            }
            lastCommand = command
        }
        return path
    }

    private static func reflect(_ point: CGPoint?, about center: CGPoint) -> CGPoint {
        guard let point else { return center }
        return CGPoint(x: 2 * center.x - point.x, y: 2 * center.y - point.y)
    }

    private enum Token {
        case command(Character)
        case number(Double)
    }

    private struct Tokens {
        private let chars: [Character]
        private var index = 0
        private var pending: Double?

        init(_ string: String) { chars = Array(string) }

        mutating func pushBack(_ value: Double) { pending = value }

        mutating func number() -> Double? {
            if let pending { self.pending = nil; return pending }
            guard case .number(let value)? = next() else { return nil }
            return value
        }

        mutating func next() -> Token? {
            if let pending { self.pending = nil; return .number(pending) }
            while index < chars.count, chars[index] == " " || chars[index] == "," || chars[index].isNewline {
                index += 1
            }
            guard index < chars.count else { return nil }
            let c = chars[index]
            if c.isLetter, c != "e", c != "E" {
                index += 1
                return .command(c)
            }
            var text = ""
            var seenDot = false
            var seenExp = false
            while index < chars.count {
                let ch = chars[index]
                if ch.isNumber {
                    text.append(ch)
                } else if (ch == "-" || ch == "+") && (text.isEmpty || text.last == "e" || text.last == "E") {
                    text.append(ch)
                } else if ch == "." && !seenDot && !seenExp {
                    seenDot = true
                    text.append(ch)
                } else if (ch == "e" || ch == "E") && !seenExp && !text.isEmpty {
                    seenExp = true
                    text.append(ch)
                } else {
                    break
                }
                index += 1
            }
            guard let value = Double(text) else {
                index += 1
                return next()
            }
            return .number(value)
        }
    }
}

/// The drawn wordmark. Lowercase, one word, never typed.
struct Wordmark: Shape {
    private static let cached = SVGPath.parse(BrandPaths.wordmark)

    static var aspectRatio: CGFloat {
        CGFloat(BrandPaths.wordmarkViewBox.2 / BrandPaths.wordmarkViewBox.3)
    }

    func path(in rect: CGRect) -> Path {
        let box = BrandPaths.wordmarkViewBox
        let scale = min(rect.width / box.2, rect.height / box.3)
        let width = box.2 * scale
        let height = box.3 * scale
        let transform = CGAffineTransform(translationX: rect.minX + (rect.width - width) / 2, y: rect.minY + (rect.height - height) / 2)
            .scaledBy(x: scale, y: scale)
            .translatedBy(x: -box.0, y: -box.1)
        return Self.cached.applying(transform)
    }
}
