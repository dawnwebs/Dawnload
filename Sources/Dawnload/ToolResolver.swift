import Foundation

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
}
