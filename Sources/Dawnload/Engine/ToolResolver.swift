import Foundation

/// The command-line tools Dawnload runs. It never bundles them; it finds them on this Mac.
enum ToolID: String, CaseIterable, Identifiable, Codable {
    case ytDlp = "yt-dlp"
    case galleryDl = "gallery-dl"
    case ffmpeg

    var id: String { rawValue }

    var purpose: String {
        switch self {
        case .ytDlp: "Video and audio from most sites"
        case .galleryDl: "Galleries and image sets"
        case .ffmpeg: "Merging, remuxing and audio"
        }
    }

    var brewFormula: String {
        switch self {
        case .ytDlp: "yt-dlp"
        case .galleryDl: "gallery-dl"
        case .ffmpeg: "ffmpeg"
        }
    }

    var versionArguments: [String] {
        switch self {
        case .ffmpeg: ["-version"]
        default: ["--version"]
        }
    }
}

struct ToolStatus: Identifiable, Equatable {
    let tool: ToolID
    let path: String?
    var version: String?

    var id: ToolID { tool }
    var isInstalled: Bool { path != nil }
}

/// The tools found at one moment, plus the environment to run them in.
struct ToolSnapshot {
    var paths: [ToolID: String]
    var environment: [String: String]

    func path(_ tool: ToolID) -> String? { paths[tool] }
}

struct ToolResolver {
    private let fileManager = FileManager.default
    private let environment: [String: String]

    init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        self.environment = environment
    }

    var searchPaths: [String] {
        var paths: [String] = [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin"
        ]

        let environmentPaths = environment["PATH"]?
            .split(separator: ":")
            .map(String.init) ?? []

        paths.append(contentsOf: environmentPaths)

        var seen = Set<String>()
        return paths.filter { seen.insert($0).inserted }
    }

    func path(for executable: String) -> String? {
        if executable.contains("/") {
            return fileManager.isExecutableFile(atPath: executable) ? executable : nil
        }

        for directory in searchPaths {
            let candidate = URL(fileURLWithPath: directory)
                .appendingPathComponent(executable)
                .path

            if fileManager.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }

        return nil
    }

    func processEnvironment() -> [String: String] {
        var result = environment
        result["PATH"] = searchPaths.joined(separator: ":")
        return result
    }

    func snapshot() -> ToolSnapshot {
        var paths: [ToolID: String] = [:]
        for tool in ToolID.allCases {
            paths[tool] = path(for: tool.rawValue)
        }
        return ToolSnapshot(paths: paths, environment: processEnvironment())
    }

    /// The tool's version, first line, trimmed: "2026.07.04", "8.0".
    func version(of tool: ToolID, at path: String) async -> String? {
        let process = ToolProcess()
        guard let result = try? await process.run(
            executable: path, arguments: tool.versionArguments, environment: processEnvironment(),
            currentDirectory: URL(fileURLWithPath: NSTemporaryDirectory()), onLine: { _ in }
        ), result.exitCode == 0 else {
            return nil
        }
        let first = result.output.components(separatedBy: .newlines).first { !$0.isEmpty } ?? ""
        if tool == .ffmpeg {
            // "ffmpeg version 8.0 Copyright (c) …"
            let words = first.split(separator: " ")
            if let index = words.firstIndex(of: "version"), index + 1 < words.count {
                return String(words[index + 1])
            }
        }
        return first.trimmingCharacters(in: .whitespaces).isEmpty ? nil : String(first.prefix(24))
    }
}
