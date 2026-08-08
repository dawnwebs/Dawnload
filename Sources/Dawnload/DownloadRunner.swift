import DawnloadCore
import Foundation

final class DownloadRunner {
    private let lock = NSLock()
    private var activeProcess: Process?

    func cancel() {
        lock.lock()
        let process = activeProcess
        lock.unlock()

        if process?.isRunning == true {
            process?.terminate()
        }
    }

    func run(_ request: DownloadRequest, event: @escaping (DownloadEvent) -> Void) async throws -> DownloadSummary {
        try FileManager.default.createDirectory(
            at: request.destination,
            withIntermediateDirectories: true
        )

        let before = mediaSnapshot(in: request.destination)
        let command = try buildCommand(for: request)

        event(.command(shellPreview(executable: command.executable, arguments: command.arguments)))
        event(.phase("Downloading"))

        let processOutput = try await runProcess(
            executable: command.executable,
            arguments: command.arguments,
            environment: request.environment,
            currentDirectory: request.destination,
            event: event
        )

        let after = mediaSnapshot(in: request.destination)
        var newFiles = after.subtracting(before).sorted { $0.path < $1.path }

        if request.mode == .ytDlp {
            newFiles = applySmartYTDlpFilenames(
                files: newFiles,
                existingFiles: after,
                metadataRecords: YTDlpMetadataParser.records(from: processOutput),
                for: request,
                event: event
            )
        }

        guard let ffmpegPath = request.ffmpegPath else {
            return DownloadSummary(finalizedFiles: 0)
        }

        event(.phase("Finishing media with ffmpeg"))
        let finalized = try await finalize(files: newFiles, ffmpegPath: ffmpegPath, environment: request.environment, event: event)

        return DownloadSummary(finalizedFiles: finalized)
    }

    private func buildCommand(for request: DownloadRequest) throws -> ProcessCommand {
        switch request.mode {
        case .ytDlp:
            guard let executable = ToolResolver(environment: request.environment).path(for: "yt-dlp") else {
                throw DownloadError.missingTool("yt-dlp")
            }

            var arguments = [
                "--newline",
                "--no-color",
                "--progress",
                "--ignore-config",
                "--no-mtime",
                "-P", request.destination.path,
                "-o", "%(title).200B [%(id)s].%(ext)s",
                "--print",
                "after_move:\(YTDlpMetadataParser.marker)%(.{id,title,fulltitle,display_id,uploader,upload_date,webpage_url,filepath})j"
            ]

            if let ffmpegPath = request.ffmpegPath {
                arguments += [
                    "--merge-output-format", "mp4",
                    "--remux-video", "mp4",
                    "--embed-metadata",
                    "--ffmpeg-location", ffmpegPath
                ]

                switch request.quality {
                case .best:
                    arguments += ["-f", "bv*+ba/b"]
                case .compatible:
                    arguments += ["-S", "res,ext:mp4:m4a", "-f", "bv*+ba/b"]
                case .audio:
                    arguments += ["-x", "--audio-format", "m4a", "--audio-quality", "0"]
                }
            } else {
                switch request.quality {
                case .best:
                    arguments += ["-f", "b"]
                case .compatible:
                    arguments += ["-S", "res,ext:mp4:m4a", "-f", "b"]
                case .audio:
                    throw DownloadError.missingTool("ffmpeg")
                }
            }

            if request.useBrowserImpersonation {
                arguments += ["--impersonate", request.impersonationTarget.rawValue]
            }

            if let cookiesFile = request.cookiesFile {
                arguments += ["--cookies", cookiesFile.path]
            }

            if request.useYesPlaylist {
                arguments.append("--yes-playlist")
            }

            arguments.append(request.url)
            return ProcessCommand(executable: executable, arguments: arguments)

        case .galleryDl:
            guard let executable = ToolResolver(environment: request.environment).path(for: "gallery-dl") else {
                throw DownloadError.missingTool("gallery-dl")
            }

            var arguments = [
                "--no-colors",
                "--no-mtime",
                "--destination", request.destination.path
            ]

            if request.ffmpegPath != nil {
                arguments += ["--ugoira", "mp4"]
            }

            arguments.append(request.url)
            return ProcessCommand(executable: executable, arguments: arguments)
        }
    }

    private func runProcess(
        executable: String,
        arguments: [String],
        environment: [String: String],
        currentDirectory: URL,
        event: @escaping (DownloadEvent) -> Void
    ) async throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.environment = environment
        process.currentDirectoryURL = currentDirectory

        let outputPipe = Pipe()
        let errorPipe = Pipe()
        let outputCollector = ProcessOutputCollector()
        process.standardOutput = outputPipe
        process.standardError = errorPipe

        outputPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            outputCollector.append(data)
            emit(data: data, event: event)
        }

        errorPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            outputCollector.append(data)
            emit(data: data, event: event)
        }

        let cancellation = CancellationFlag()

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let completion = ProcessCompletion()

                process.terminationHandler = { process in
                    outputPipe.fileHandleForReading.readabilityHandler = nil
                    errorPipe.fileHandleForReading.readabilityHandler = nil

                    for handle in [
                        outputPipe.fileHandleForReading,
                        errorPipe.fileHandleForReading
                    ] {
                        let remainingData = handle.readDataToEndOfFile()
                        guard !remainingData.isEmpty else { continue }
                        outputCollector.append(remainingData)
                        emit(data: remainingData, event: event)
                    }

                    completion.resume {
                        self.clearActiveProcess(process)

                        if process.terminationStatus == 0 {
                            continuation.resume(returning: outputCollector.text)
                        } else if cancellation.isCancelled {
                            continuation.resume(throwing: CancellationError())
                        } else {
                            continuation.resume(throwing: DownloadError.processFailed(
                                executable: URL(fileURLWithPath: executable).lastPathComponent,
                                code: process.terminationStatus,
                                output: outputCollector.text
                            ))
                        }
                    }
                }

                do {
                    self.setActiveProcess(process)
                    try process.run()
                } catch {
                    self.clearActiveProcess(process)
                    outputPipe.fileHandleForReading.readabilityHandler = nil
                    errorPipe.fileHandleForReading.readabilityHandler = nil
                    completion.resume {
                        continuation.resume(throwing: error)
                    }
                }
            }
        } onCancel: {
            cancellation.cancel()
            self.cancel()
        }
    }

    private func finalize(
        files: [URL],
        ffmpegPath: String,
        environment: [String: String],
        event: @escaping (DownloadEvent) -> Void
    ) async throws -> Int {
        var count = 0

        for file in files where shouldOptimize(file) {
            let temp = temporaryOptimizedURL(for: file)
            event(.line("Optimizing \(file.lastPathComponent)"))

            do {
                _ = try await runProcess(
                    executable: ffmpegPath,
                    arguments: [
                        "-y",
                        "-i", file.path,
                        "-map", "0",
                        "-c", "copy",
                        "-movflags", "+faststart",
                        temp.path
                    ],
                    environment: environment,
                    currentDirectory: file.deletingLastPathComponent(),
                    event: event
                )

                try replace(file: file, with: temp)
                count += 1
            } catch {
                try? FileManager.default.removeItem(at: temp)
                event(.line("Skipped ffmpeg optimization for \(file.lastPathComponent): \(error.localizedDescription)"))
            }
        }

        return count
    }

    private func mediaSnapshot(in directory: URL) -> Set<URL> {
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]
        ) else {
            return []
        }

        var result = Set<URL>()
        for case let file as URL in enumerator {
            guard isMediaFile(file) else { continue }
            result.insert(file.standardizedFileURL)
        }

        return result
    }

    private func applySmartYTDlpFilenames(
        files: [URL],
        existingFiles: Set<URL>,
        metadataRecords: [SmartFilenameMetadata],
        for request: DownloadRequest,
        event: @escaping (DownloadEvent) -> Void
    ) -> [URL] {
        guard !metadataRecords.isEmpty else {
            return files
        }

        let fileManager = FileManager.default
        var reservedFiles = existingFiles
        var remainingMetadata = metadataRecords
        var renamedFiles: [URL] = []

        for file in files {
            let standardizedFile = file.standardizedFileURL
            reservedFiles.remove(standardizedFile)

            guard let metadataIndex = matchingMetadataIndex(
                for: standardizedFile,
                in: remainingMetadata,
                singleFileFallback: files.count == 1 && remainingMetadata.count == 1
            ) else {
                reservedFiles.insert(standardizedFile)
                renamedFiles.append(standardizedFile)
                continue
            }

            let metadata = remainingMetadata.remove(at: metadataIndex)
            let resolvedTitle = SmartFilenameResolver.bestTitle(
                metadata: metadata,
                sourceURL: request.url
            )
            let replacementName = limitedFileNameComponent(resolvedTitle, byteLimit: 180)

            guard !replacementName.isEmpty else {
                reservedFiles.insert(standardizedFile)
                renamedFiles.append(standardizedFile)
                continue
            }

            let folder = standardizedFile.deletingLastPathComponent()
            let filename = standardizedFile.pathExtension.isEmpty
                ? replacementName
                : "\(replacementName).\(standardizedFile.pathExtension)"
            let desired = folder.appendingPathComponent(filename)

            guard desired.standardizedFileURL != standardizedFile else {
                reservedFiles.insert(standardizedFile)
                renamedFiles.append(standardizedFile)
                continue
            }

            let target = uniqueURL(for: desired, reservedFiles: reservedFiles)

            do {
                try fileManager.moveItem(at: standardizedFile, to: target)
                event(.line("Smart filename: \(target.lastPathComponent)"))
                reservedFiles.insert(target.standardizedFileURL)
                renamedFiles.append(target.standardizedFileURL)
            } catch {
                event(.line("Could not rename \(standardizedFile.lastPathComponent): \(error.localizedDescription)"))
                reservedFiles.insert(standardizedFile)
                renamedFiles.append(standardizedFile)
            }
        }

        return renamedFiles.sorted { $0.path < $1.path }
    }

    private func matchingMetadataIndex(
        for file: URL,
        in metadataRecords: [SmartFilenameMetadata],
        singleFileFallback: Bool
    ) -> Int? {
        if let exactIndex = metadataRecords.firstIndex(where: { metadata in
            guard let filepath = metadata.filepath else {
                return false
            }

            return URL(fileURLWithPath: filepath).standardizedFileURL == file
        }) {
            return exactIndex
        }

        let stemKey = normalizedTitleKey(file.deletingPathExtension().lastPathComponent)
        if let identifierIndex = metadataRecords.firstIndex(where: { metadata in
            let idKey = normalizedTitleKey(metadata.id)
            return !idKey.isEmpty && stemKey.contains(idKey)
        }) {
            return identifierIndex
        }

        return singleFileFallback ? metadataRecords.indices.first : nil
    }

    private func uniqueURL(for desired: URL, reservedFiles: Set<URL>) -> URL {
        let fileManager = FileManager.default

        func isAvailable(_ url: URL) -> Bool {
            !reservedFiles.contains(url.standardizedFileURL)
                && !fileManager.fileExists(atPath: url.path)
        }

        if isAvailable(desired) {
            return desired
        }

        let folder = desired.deletingLastPathComponent()
        let stem = desired.deletingPathExtension().lastPathComponent
        let fileExtension = desired.pathExtension
        var index = 2

        while true {
            let filename = fileExtension.isEmpty
                ? "\(stem) \(index)"
                : "\(stem) \(index).\(fileExtension)"
            let candidate = folder.appendingPathComponent(filename)

            if isAvailable(candidate) {
                return candidate
            }

            index += 1
        }
    }

    private func isMediaFile(_ url: URL) -> Bool {
        let ext = url.pathExtension.lowercased()
        return [
            "mp4", "m4v", "mov", "webm", "mkv", "avi", "flv", "mp3", "m4a", "aac", "opus"
        ].contains(ext)
    }

    private func shouldOptimize(_ url: URL) -> Bool {
        ["mp4", "m4v", "mov"].contains(url.pathExtension.lowercased())
    }

    private func temporaryOptimizedURL(for url: URL) -> URL {
        let folder = url.deletingLastPathComponent()
        let name = url.deletingPathExtension().lastPathComponent
        let ext = url.pathExtension
        return folder.appendingPathComponent(".\(name).dawnload-optimized.\(ext)")
    }

    private func replace(file: URL, with replacement: URL) throws {
        let backup = file.appendingPathExtension("dawnload-backup")
        let fileManager = FileManager.default

        try? fileManager.removeItem(at: backup)
        try fileManager.moveItem(at: file, to: backup)

        do {
            try fileManager.moveItem(at: replacement, to: file)
            try? fileManager.removeItem(at: backup)
        } catch {
            try? fileManager.moveItem(at: backup, to: file)
            throw error
        }
    }

    private func setActiveProcess(_ process: Process) {
        lock.lock()
        activeProcess = process
        lock.unlock()
    }

    private func clearActiveProcess(_ process: Process) {
        lock.lock()
        if activeProcess === process {
            activeProcess = nil
        }
        lock.unlock()
    }
}

private func emit(data: Data, event: @escaping (DownloadEvent) -> Void) {
    guard let text = String(data: data, encoding: .utf8), !text.isEmpty else {
        return
    }

    for line in text.components(separatedBy: .newlines)
    where !line.isEmpty && !line.contains(YTDlpMetadataParser.marker) {
        event(.line(line))
        event(.progress(progressValue(from: line)))
    }
}

private func progressValue(from line: String) -> Double? {
    guard let range = line.range(of: #"([0-9]{1,3}(?:\.[0-9]+)?)%"#, options: .regularExpression) else {
        return nil
    }

    let percent = line[range].dropLast()
    guard let value = Double(percent) else {
        return nil
    }

    return min(max(value / 100, 0), 1)
}

private func safeFileNameComponent(_ value: String) -> String {
    let forbidden = CharacterSet(charactersIn: "/:")
        .union(.controlCharacters)
        .union(.newlines)

    let sanitizedScalars = value.unicodeScalars.map { scalar in
        forbidden.contains(scalar) ? " " : String(scalar)
    }
    let collapsed = sanitizedScalars
        .joined()
        .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .trimmingCharacters(in: CharacterSet(charactersIn: "."))

    if collapsed == "." || collapsed == ".." {
        return ""
    }

    return collapsed
}

private func limitedFileNameComponent(_ value: String, byteLimit: Int) -> String {
    var result = safeFileNameComponent(value)

    while result.utf8.count > byteLimit && !result.isEmpty {
        result.removeLast()
    }

    return result.trimmingCharacters(in: .whitespacesAndNewlines)
}

private func normalizedTitleKey(_ value: String?) -> String {
    guard let value else {
        return ""
    }

    let lowercased = value.lowercased()
    return String(lowercased.unicodeScalars.filter {
        CharacterSet.alphanumerics.contains($0)
    })
}

private func shellPreview(executable: String, arguments: [String]) -> String {
    ([executable] + arguments)
        .map(shellEscape)
        .joined(separator: " ")
}

private func shellEscape(_ value: String) -> String {
    if value.range(of: #"^[A-Za-z0-9_/\-.:=%+]+$"#, options: .regularExpression) != nil {
        return value
    }

    return "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
}

private struct ProcessCommand {
    let executable: String
    let arguments: [String]
}

private final class CancellationFlag {
    private let lock = NSLock()
    private var cancelled = false

    var isCancelled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelled
    }

    func cancel() {
        lock.lock()
        cancelled = true
        lock.unlock()
    }
}

private final class ProcessCompletion {
    private let lock = NSLock()
    private var didResume = false

    func resume(_ block: () -> Void) {
        lock.lock()
        defer { lock.unlock() }

        guard !didResume else { return }
        didResume = true
        block()
    }
}

private final class ProcessOutputCollector {
    private let lock = NSLock()
    private var data = Data()
    private let byteLimit = 64 * 1024

    func append(_ newData: Data) {
        lock.lock()
        defer { lock.unlock() }

        data.append(newData)
        if data.count > byteLimit {
            data.removeFirst(data.count - byteLimit)
        }
    }

    var text: String {
        lock.lock()
        defer { lock.unlock() }
        return String(data: data, encoding: .utf8) ?? ""
    }
}

enum DownloadError: LocalizedError {
    case missingTool(String)
    case processFailed(executable: String, code: Int32, output: String)

    var errorDescription: String? {
        switch self {
        case .missingTool(let tool):
            return "\(tool) is not installed or could not be found."
        case .processFailed(let executable, let code, let output):
            if output.localizedCaseInsensitiveContains("HTTP Error 403")
                || output.localizedCaseInsensitiveContains("403: Forbidden") {
                return "HTTP 403: the site refused this request. Try Chrome 146, then Chrome 145/142 or Safari 18.4. If it still fails, select a cookies.txt file in Settings."
            }

            return "\(executable) exited with code \(code)."
        }
    }
}
