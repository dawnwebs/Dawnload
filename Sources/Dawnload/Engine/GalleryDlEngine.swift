import DawnloadCore
import Foundation

struct GalleryDlEngine: DownloadEngine {
    let id = EngineID.galleryDl
    let displayName = "gallery-dl"
    let lane = EngineLane.web
    let capabilities: EngineCapabilities = [.polish, .resume]

    func accepts(_ source: DownloadSource) -> Bool {
        source.kind == .web
    }

    func availability(tools: ToolSnapshot, options: DownloadOptions) -> EngineAvailability {
        if tools.path(.galleryDl) == nil { return .missingTool(.galleryDl) }
        return .ready
    }

    func makeJob(for request: JobRequest) throws -> DownloadJob {
        guard case .web(let url) = request.source else { throw DownloadFailure.torrentsUnavailable }
        guard let executable = request.tools.path(.galleryDl) else { throw DownloadFailure.toolMissing("gallery-dl") }
        return GalleryDlJob(url: url, executable: executable, request: request)
    }

    static func arguments(url: URL, options: DownloadOptions, ffmpegAvailable: Bool) -> [String] {
        var arguments = ["--no-colors", "--no-mtime", "--destination", options.destination.path]
        if options.polish, ffmpegAvailable {
            arguments += ["--ugoira", "mp4"]
        }
        arguments.append(url.absoluteString)
        return arguments
    }
}

private final class GalleryDlJob: DownloadJob {
    private let url: URL
    private let executable: String
    private let request: JobRequest
    private let process = ToolProcess()

    init(url: URL, executable: String, request: JobRequest) {
        self.url = url
        self.executable = executable
        self.request = request
    }

    func stop() {
        process.stop()
    }

    func run(emit: @escaping (JobEvent) -> Void) async throws -> JobOutcome {
        let options = request.options
        let destination = options.destination
        do {
            try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        } catch {
            throw DownloadFailure.folderUnavailable(destination.path)
        }

        let ffmpegPath = request.tools.path(.ffmpeg)
        let arguments = GalleryDlEngine.arguments(url: url, options: options, ffmpegAvailable: ffmpegPath != nil)
        emit(.command(shellPreview(executable: executable, arguments: arguments)))
        emit(.phase(.pulling, detail: nil))

        let collected = FileList()
        let destinationPath = destination.standardizedFileURL.path

        let result = try await process.run(
            executable: executable,
            arguments: arguments,
            environment: request.tools.environment,
            currentDirectory: destination
        ) { line in
            // gallery-dl prints each file it saves as a path, and "# path" for ones already there.
            let path = line.hasPrefix("# ") ? String(line.dropFirst(2)) : line
            if path.hasPrefix("/") {
                let url = URL(fileURLWithPath: path).standardizedFileURL
                let count = collected.add(url)
                if count == 1 {
                    emit(.title(Self.title(for: url, under: destinationPath)))
                }
                emit(.files(FileFinishing.itemFiles(collected.all)))
                emit(.transfer(TransferSample(bytesDone: collected.totalBytes)))
                emit(.phase(.pulling, detail: DLFormat.count(count, "file")))
            } else {
                emit(.log(line))
            }
        }

        let files = collected.all
        if result.exitCode != 0 {
            guard !files.isEmpty else {
                throw DownloadFailure.classify(tool: "gallery-dl", exitCode: result.exitCode, output: result.output)
            }
            emit(.log("gallery-dl finished with exit \(result.exitCode); \(DLFormat.count(files.count, "file")) were kept."))
        }

        var polished = 0
        if options.polish, let ffmpegPath {
            polished = try await FileFinishing.polish(
                files: files, ffmpegPath: ffmpegPath, environment: request.tools.environment,
                process: process, emit: emit
            )
        }

        return JobOutcome(files: files, polishedCount: polished)
    }

    /// "instagram / someone" from …/Dawnload/instagram/someone/file.jpg.
    static func title(for file: URL, under destination: String) -> String {
        let folder = file.deletingLastPathComponent().path
        guard folder.hasPrefix(destination) else { return file.deletingLastPathComponent().lastPathComponent }
        let relative = folder.dropFirst(destination.count).split(separator: "/").prefix(2)
        return relative.isEmpty ? file.lastPathComponent : relative.joined(separator: " / ")
    }
}

private final class FileList {
    private let lock = NSLock()
    private var urls: [URL] = []
    private var seen = Set<URL>()

    func add(_ url: URL) -> Int {
        lock.lock()
        defer { lock.unlock() }
        if seen.insert(url).inserted { urls.append(url) }
        return urls.count
    }

    var all: [URL] {
        lock.lock()
        defer { lock.unlock() }
        return urls
    }

    var totalBytes: Int64 {
        all.compactMap(FileFinishing.size(of:)).reduce(0, +)
    }
}
