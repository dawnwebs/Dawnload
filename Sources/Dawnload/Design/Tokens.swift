import AppKit
import SwiftUI

// Brand tokens for the app, from docs/brand/web/dawnload-tokens.json (green colourway).
// Roles, not hexes: views use DLColor.text, never a hex. See docs/DESIGN.md › Colour.

enum DLPalette {
    static let signal = NSColor(hex: 0x17D36E)
    static let pine = NSColor(hex: 0x07140E)
    static let grove = NSColor(hex: 0x0F2219)
    static let mossLine = NSColor(hex: 0x1B3428)
    static let sage = NSColor(hex: 0x9DC4AE)
    static let paper = NSColor(hex: 0xF2F8F3)
    static let dew = NSColor(hex: 0xDCE9DF)
    static let lichen = NSColor(hex: 0x4C6B5A)
    static let forest = NSColor(hex: 0x0B7A3E)
    static let coral = NSColor(hex: 0xFF6B5C)
    static let coralDeep = NSColor(hex: 0xB8261A)
    static let white = NSColor(hex: 0xFFFFFF)
}

enum DLColor {
    /// The window.
    static let canvas = role(dark: DLPalette.pine, light: DLPalette.paper)
    /// The paste field, the inspector, selected rows, chips that are on.
    static let raised = role(dark: DLPalette.grove, light: DLPalette.white)
    /// One step above raised: hover on rows and chips.
    static let raisedHover = role(dark: NSColor(hex: 0x142C20), light: NSColor(hex: 0xE8F1EA))
    /// Hairlines and outlines. Stronger under Increase Contrast.
    static let rule = role(dark: DLPalette.mossLine, light: DLPalette.dew,
                           darkContrast: DLPalette.sage, lightContrast: DLPalette.lichen)
    /// The empty part of the D and of any track.
    static let track = role(dark: DLPalette.mossLine, light: DLPalette.dew)
    static let text = role(dark: DLPalette.paper, light: DLPalette.pine)
    static let text2 = role(dark: DLPalette.sage, light: DLPalette.lichen)
    /// The Download button. One per view.
    static let action = role(dark: DLPalette.signal, light: DLPalette.signal)
    /// Text and glyphs on Signal: always Pine, never white.
    static let onAction = role(dark: DLPalette.pine, light: DLPalette.pine)
    /// Links, focus, toggles that are on, the selection spine.
    static let link = role(dark: DLPalette.signal, light: DLPalette.forest)
    static let progress = role(dark: DLPalette.signal, light: DLPalette.signal)
    static let kept = role(dark: DLPalette.signal, light: DLPalette.forest)
    static let failed = role(dark: DLPalette.coral, light: DLPalette.coralDeep)

    static var nsLink: NSColor { dynamic(dark: DLPalette.signal, light: DLPalette.forest) }
    static var nsCanvas: NSColor { dynamic(dark: DLPalette.pine, light: DLPalette.paper) }

    private static func role(dark: NSColor, light: NSColor, darkContrast: NSColor? = nil, lightContrast: NSColor? = nil) -> Color {
        Color(nsColor: dynamic(dark: dark, light: light, darkContrast: darkContrast, lightContrast: lightContrast))
    }

    static func dynamic(dark: NSColor, light: NSColor, darkContrast: NSColor? = nil, lightContrast: NSColor? = nil) -> NSColor {
        NSColor(name: nil) { appearance in
            let match = appearance.bestMatch(from: [
                .darkAqua, .aqua, .accessibilityHighContrastDarkAqua, .accessibilityHighContrastAqua
            ])
            switch match {
            case .accessibilityHighContrastDarkAqua: return darkContrast ?? dark
            case .accessibilityHighContrastAqua: return lightContrast ?? light
            case .darkAqua: return dark
            default: return light
            }
        }
    }
}

enum DLFont {
    static let displayName = "Unbounded-ExtraBold"
    static let monoName = "MartianMono-Medium"
    static let monoBoldName = "MartianMono-Bold"

    /// Unbounded ExtraBold. The one headline or big number in a window.
    static func display(_ size: CGFloat) -> Font {
        FontRegistry.isAvailable(displayName)
            ? .custom(displayName, fixedSize: size)
            : .system(size: size, weight: .heavy, design: .rounded)
    }

    /// Martian Mono: URLs, file names, sizes, speeds, logs.
    static func mono(_ size: CGFloat, bold: Bool = false) -> Font {
        let name = bold ? monoBoldName : monoName
        return FontRegistry.isAvailable(name)
            ? .custom(name, fixedSize: size)
            : .system(size: size, weight: bold ? .bold : .medium, design: .monospaced)
    }

    static func nsMono(_ size: CGFloat) -> NSFont {
        NSFont(name: monoName, size: size) ?? .monospacedSystemFont(ofSize: size, weight: .medium)
    }

    /// Display tracking, −0.02 em.
    static func displayTracking(_ size: CGFloat) -> CGFloat { -0.02 * size }
}

enum DLRadius {
    /// The D's small corner: fields, rows, file rows, the log.
    static let spine: CGFloat = 6
    /// Cards and sheets.
    static let card: CGFloat = 20
    /// The D's round side: buttons, chips, tabs. Use Capsule().
    static let pill: CGFloat = 999
}

enum DLSpace {
    static let xs: CGFloat = 4
    static let s: CGFloat = 8
    static let m: CGFloat = 12
    static let l: CGFloat = 16
    static let xl: CGFloat = 24
    static let xxl: CGFloat = 32
    static let gutter: CGFloat = 24
}

enum DLMotion {
    /// The arrow presses down: 260 ms, cubic-bezier(0.3, 0, 0, 1).
    static let pull = Animation.timingCurve(0.3, 0, 0, 1, duration: 0.26)
    /// Things arriving: 520 ms, cubic-bezier(0.16, 1, 0.3, 1).
    static let settle = Animation.timingCurve(0.16, 1, 0.3, 1, duration: 0.52)
    /// The fill between progress samples.
    static let fill = Animation.linear(duration: 0.2)
    /// State changes cross-fade.
    static let swap = Animation.easeOut(duration: 0.18)
    /// One pass of the loader.
    static let loaderPeriod: Double = 1.4
}

extension NSColor {
    convenience init(hex: UInt32, alpha: CGFloat = 1) {
        self.init(
            srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: alpha
        )
    }
}
