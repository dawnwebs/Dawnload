#!/usr/bin/env swift

// Builds AppIcon.iconset from the brand kit's app icon (green colourway):
// docs/brand/app-icon/png/dawnload-app-icon-<size>.png, drawn on Apple's 1024 grid.
// The brand kit renders 16, 32 and 64 px by hand for crispness; any size it doesn't have is
// resampled from the 1024 master.
//
// Usage: make-icon.swift OUTPUT.iconset [SOURCE_PNG_FOLDER]

import AppKit
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 2 || arguments.count == 3 else {
    fputs("Usage: make-icon.swift OUTPUT.iconset [SOURCE_PNG_FOLDER]\n", stderr)
    exit(64)
}

let scriptURL = URL(fileURLWithPath: arguments[0]).standardizedFileURL
let root = scriptURL.deletingLastPathComponent().deletingLastPathComponent()
let sourceFolder = arguments.count == 3
    ? URL(fileURLWithPath: arguments[2], isDirectory: true)
    : root.appendingPathComponent("docs/brand/app-icon/png", isDirectory: true)
let outputURL = URL(fileURLWithPath: arguments[1], isDirectory: true)

func source(_ pixels: Int) -> URL {
    sourceFolder.appendingPathComponent("dawnload-app-icon-\(pixels).png")
}

guard FileManager.default.fileExists(atPath: source(1024).path) else {
    fputs("Missing \(source(1024).path). Rebuild the brand kit (docs/brand/README.md › Rebuild).\n", stderr)
    exit(66)
}

try FileManager.default.createDirectory(at: outputURL, withIntermediateDirectories: true)

let icons: [(name: String, pixels: Int)] = [
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

func resampled(from master: URL, pixels: Int) throws -> Data {
    guard let image = NSImage(contentsOf: master),
          let bitmap = NSBitmapImageRep(
              bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
              bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
              colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
          ) else {
        throw NSError(domain: "make-icon", code: 1, userInfo: [NSLocalizedDescriptionKey: "Could not read \(master.path)"])
    }
    bitmap.size = NSSize(width: pixels, height: pixels)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    NSGraphicsContext.current?.imageInterpolation = .high
    image.draw(in: NSRect(x: 0, y: 0, width: pixels, height: pixels))
    NSGraphicsContext.restoreGraphicsState()
    guard let png = bitmap.representation(using: .png, properties: [:]) else {
        throw NSError(domain: "make-icon", code: 2, userInfo: [NSLocalizedDescriptionKey: "Could not encode \(pixels) px"])
    }
    return png
}

for icon in icons {
    let target = outputURL.appendingPathComponent(icon.name)
    try? FileManager.default.removeItem(at: target)
    let exact = source(icon.pixels)
    if FileManager.default.fileExists(atPath: exact.path) {
        try FileManager.default.copyItem(at: exact, to: target)
    } else {
        try resampled(from: source(1024), pixels: icon.pixels).write(to: target)
    }
}

print("Wrote \(icons.count) icons from \(sourceFolder.path)")
