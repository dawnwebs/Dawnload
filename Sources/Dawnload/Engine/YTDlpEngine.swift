import DawnloadCore
import Foundation

struct YTDlpEngine: DownloadEngine {
    let id = EngineID.ytDlp
    let displayName = "yt-dlp"
    let lane = EngineLane.web
    let capabilities: EngineCapabilities = [.formats, .playlists, .siteAccess, .polish, .resume]

    func accepts(_ source: DownloadSource) -> Bool {
        source.kind == .web
    }

    func availability(tools: ToolSnapshot, options: DownloadOptions) -> EngineAvailability {
        if tools.path(.ytDlp) == nil { return .missingTool(.ytDlp) }
        if (options.polish || options.quality == .audio), tools.path(.ffmpeg) == nil { return .missingTool(.ffmpeg) }
        return .ready
    }

    func makeJob(for request: JobRequest) throws -> DownloadJob {
        guard case .web(let url) = request.source else { throw DownloadFailure.torrentsUnavailable }
        guard let executable = request.tools.path(.ytDlp) else { throw DownloadFailure.toolMissing("yt-dlp") }
        if request.options.quality == .audio, request.tools.path(.ffmpeg) == nil { throw DownloadFailure.ffmpegNeeded }
        if request.options.polish, request.tools.path(.ffmpeg) == nil { throw DownloadFailure.toolMissing("ffmpeg") }
        if request.options.useCookies, let cookies = request.options.cookiesFile,
           !FileManager.default.isReadableFile(atPath: cookies.path) {
            throw DownloadFailure.cookiesUnreadable
        }
        return YTDlpJob(url: url, executable: executable, request: request)
    }

    static func arguments(url: URL, options: DownloadOptions, ffmpegPath: String?) -> [String] {
        var arguments = [
            "--newline",
            "--no-color",
            "--progress",
            "--ignore-config",
            "--no-update",
            "--no-mtime",
            "-P", options.destination.path,
            "-o", "%(title).200B [%(id)s].%(ext)s",
            "--print",
            "after_move:\(YTDlpMetadataParser.marker)%(.{id,title,fulltitle,display_id,uploader,upload_date,webpage_url,filepath})j"
        ]
        arguments += YTDlpMarkers.arguments

        let polish = options.polish || options.quality == .audio
        if polish, let ffmpegPath {
            arguments += [
                "--merge-output-format", "mp4",
                "--remux-video", "mp4",
                "--embed-metadata",
                "--ffmpeg-location", ffmpegPath
            ]

            switch options.quality {
            case .best:
                arguments += ["-f", "bv*+ba/b"]
            case .compatible:
                arguments += ["-S", "res,ext:mp4:m4a", "-f", "bv*+ba/b"]
            case .audio:
                arguments += ["-x", "--audio-format", "m4a", "--audio-quality", "0"]
            }
        } else {
            switch options.quality {
            case .best, .audio:
                arguments += ["-f", "b"]
            case .compatible:
                arguments += ["-S", "res,ext:mp4:m4a", "-f", "b"]
            }
        }

        if options.impersonate {
            arguments += ["--impersonate", options.impersonationTarget.rawValue]
        }

        if options.useCookies, let cookiesFile = options.cookiesFile {
            arguments += ["--cookies", cookiesFile.path]
        }

        arguments.append(options.playlist ? "--yes-playlist" : "--no-playlist")
        arguments.append(url.absoluteString)
        return arguments
    }
}

private final class YTDlpJob: DownloadJob {
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
        let arguments = YTDlpEngine.arguments(url: url, options: options, ffmpegPath: ffmpegPath)
        let before = FileFinishing.snapshot(in: destination, extensions: FileFinishing.mediaExtensions)

        emit(.command(shellPreview(executable: executable, arguments: arguments)))
        emit(.phase(.pulling, detail: nil))

        let tracker = TrackerBox()
        let result = try await process.run(
            executable: executable,
            arguments: arguments,
            environment: request.tools.environment,
            currentDirectory: destination
        ) { line in
            let parsed = YTDlpLine.parse(line)
            switch parsed {
            case .item(let info):
                _ = tracker.consume(parsed)
                if let title = info.title { emit(.title(title)) }
                if let index = info.playlistIndex, let count = info.entryCount {
                    emit(.log("Video \(index) of \(count): \(info.title ?? info.id ?? "")"))
                }
                emit(.phase(.pulling, detail: nil))
            case .parts:
                _ = tracker.consume(parsed)
            case .progress:
                if let sample = tracker.consume(parsed) { emit(.transfer(sample)) }
            case .postprocess(let started, let name, let isPolish):
                if started, isPolish {
                    emit(.phase(.polishing, detail: YTDlpLine.polishDescription(for: name)))
                    emit(.log("Polishing: \(name)"))
                }
            case .metadata:
                break
            case .text(let text):
                emit(.log(text))
            }
        }

        guard result.exitCode == 0 else {
            throw DownloadFailure.classify(tool: "yt-dlp", exitCode: result.exitCode, output: result.output)
        }

        let after = FileFinishing.snapshot(in: destination, extensions: FileFinishing.mediaExtensions)
        let metadata = YTDlpMetadataParser.records(from: result.output)
        var newFiles = after.subtracting(before).sorted { $0.path < $1.path }

        if newFiles.isEmpty {
            // Already on disk: yt-dlp skipped the download. The files it reported are still this item's.
            newFiles = metadata.compactMap(\.filepath).map { URL(fileURLWithPath: $0).standardizedFileURL }
                .filter { FileManager.default.fileExists(atPath: $0.path) }
            if !newFiles.isEmpty { emit(.log("Already in the folder; nothing new to download.")) }
        } else {
            newFiles = FileFinishing.applySmartNames(
                files: newFiles, existingFiles: after, metadataRecords: metadata,
                sourceURL: url.absoluteString, log: { emit(.log($0)) }
            )
        }

        if newFiles.count == 1, let name = newFiles.first?.deletingPathExtension().lastPathComponent {
            emit(.title(name))
        }

        var polished = 0
        if options.polish, let ffmpegPath {
            polished = try await FileFinishing.polish(
                files: newFiles, ffmpegPath: ffmpegPath, environment: request.tools.environment,
                process: process, emit: emit
            )
        }

        return JobOutcome(files: newFiles, polishedCount: polished)
    }
}

/// The tracker is a value type mutated from the pipe's thread; this keeps it in one place.
private final class TrackerBox {
    private let lock = NSLock()
    private var tracker = YTDlpProgressTracker()

    func consume(_ line: YTDlpLine) -> TransferSample? {
        lock.lock()
        defer { lock.unlock() }
        return tracker.consume(line)
    }
}
