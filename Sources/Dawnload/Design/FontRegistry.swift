import AppKit
import CoreText

/// Registers the bundled OFL fonts (Unbounded, Martian Mono) for this process.
/// The packaged app carries them in Contents/Resources/Fonts; `swift run` reads them from the
/// SwiftPM resource bundle.
enum FontRegistry {
    private static var registered = Set<String>()

    static func registerBundledFonts() {
        guard let folder = fontsFolder() else { return }
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        for file in files where file.pathExtension.lowercased() == "ttf" {
            var error: Unmanaged<CFError>?
            CTFontManagerRegisterFontsForURL(file as CFURL, .process, &error)
        }
        for name in [DLFont.displayName, DLFont.monoName, DLFont.monoBoldName] where NSFont(name: name, size: 12) != nil {
            registered.insert(name)
        }
    }

    static func isAvailable(_ postScriptName: String) -> Bool {
        registered.contains(postScriptName)
    }

    private static func fontsFolder() -> URL? {
        if let url = Bundle.main.resourceURL?.appendingPathComponent("Fonts"),
           FileManager.default.fileExists(atPath: url.path) {
            return url
        }
        // SwiftPM's resource bundle sits next to the executable for `swift run`. Look for it
        // by hand: Bundle.module traps when the bundle is missing.
        let executableFolder = Bundle.main.executableURL?.deletingLastPathComponent()
        let candidates = [
            executableFolder?.appendingPathComponent("Dawnload_Dawnload.bundle"),
            Bundle.main.resourceURL?.appendingPathComponent("Dawnload_Dawnload.bundle")
        ].compactMap { $0 }
        for bundleURL in candidates {
            for sub in ["Fonts", "Contents/Resources/Fonts"] {
                let url = bundleURL.appendingPathComponent(sub)
                if FileManager.default.fileExists(atPath: url.path) { return url }
            }
        }
        return nil
    }
}
