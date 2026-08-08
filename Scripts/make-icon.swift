#!/usr/bin/env swift

import AppKit
import Foundation

guard CommandLine.arguments.count == 2 else {
    fputs("Usage: make-icon.swift OUTPUT.iconset\n", stderr)
    exit(64)
}

let outputURL = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: outputURL, withIntermediateDirectories: true)

let icons: [(name: String, pixels: CGFloat)] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024)
]

for icon in icons {
    let image = NSImage(size: NSSize(width: icon.pixels, height: icon.pixels))
    image.lockFocus()

    drawIcon(size: icon.pixels)

    image.unlockFocus()

    guard
        let tiff = image.tiffRepresentation,
        let bitmap = NSBitmapImageRep(data: tiff),
        let png = bitmap.representation(using: .png, properties: [:])
    else {
        fputs("Could not render \(icon.name)\n", stderr)
        exit(1)
    }

    try png.write(to: outputURL.appendingPathComponent(icon.name))
}

private func drawIcon(size: CGFloat) {
    let rect = NSRect(x: 0, y: 0, width: size, height: size)
    NSColor.clear.setFill()
    rect.fill()

    let inset = size * 0.055
    let iconRect = rect.insetBy(dx: inset, dy: inset)
    let radius = size * 0.225
    let shape = NSBezierPath(roundedRect: iconRect, xRadius: radius, yRadius: radius)

    NSGraphicsContext.current?.saveGraphicsState()
    shape.addClip()

    let gradient = NSGradient(colors: [
        NSColor.controlAccentColor,
        NSColor.systemBlue.blended(withFraction: 0.2, of: .white) ?? .systemBlue
    ])
    gradient?.draw(in: shape, angle: -42)

    let glow = NSBezierPath(ovalIn: NSRect(
        x: size * 0.08,
        y: size * 0.52,
        width: size * 0.82,
        height: size * 0.62
    ))
    (NSColor.white.withAlphaComponent(0.22)).setFill()
    glow.fill()

    NSGraphicsContext.current?.restoreGraphicsState()

    let stroke = max(2, size * 0.072)
    let centerX = size * 0.5
    let tipY = size * 0.29
    let topY = size * 0.68
    let wingY = size * 0.45
    let wingX = size * 0.18

    let arrow = NSBezierPath()
    arrow.lineWidth = stroke
    arrow.lineCapStyle = .round
    arrow.lineJoinStyle = .round
    arrow.move(to: NSPoint(x: centerX, y: topY))
    arrow.line(to: NSPoint(x: centerX, y: tipY))
    arrow.move(to: NSPoint(x: centerX - wingX, y: wingY))
    arrow.line(to: NSPoint(x: centerX, y: tipY))
    arrow.line(to: NSPoint(x: centerX + wingX, y: wingY))

    NSColor.white.setStroke()
    arrow.stroke()

    let tray = NSBezierPath()
    tray.lineWidth = stroke
    tray.lineCapStyle = .round
    tray.move(to: NSPoint(x: size * 0.28, y: size * 0.22))
    tray.line(to: NSPoint(x: size * 0.72, y: size * 0.22))
    tray.stroke()
}
