import DawnloadCore
import Foundation

/// What happens to files after a web engine pulls them: find the new ones, give yt-dlp's files
/// smart names, and polish MP4s with ffmpeg (faststart).
enum FileFinishing {
    static let mediaExtensions: Set<String> = [
        "mp4", "m4v", "mov", "webm", "mkv", "avi", "flv", "mp3", "m4a", "aac", "opus", "ogg", "flac", "wav"
    ]

    static let imageExtensions: Set<String> = [
        "jpg", "jpeg", "png", "gif", "webp", "heic", "avif", "bmp", "tiff", "svg", "zip"
    ]

    static func snapshot(in directory: URL, extensions: Set<String>) -> Set<URL> {
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]
        ) else {
            return []
        }

        var result = Set<URL>()
        for case let file as URL in enumerator where extensions.contains(file.pathExtension.lowercased()) {
            result.insert(file.standardizedFileURL)
        }
        return result
    }

    static func size(of url: URL) -> Int64? {
        (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(Int64.init)
    }

    static func itemFiles(_ urls: [URL]) -> [ItemFile] {
        urls.map { ItemFile(path: $0.path, size: size(of: $0), bytesDone: size(of: $0)) }
    }

    // MARK: Smart filenames

    static func applySmartNames(
        files: [URL],
        existingFiles: Set<URL>,
        metadataRecords: [SmartFilenameMetadata],
        sourceURL: String,
        log: (String) -> Void
    ) -> [URL] {
        guard !metadataRecords.isEmpty else { return files }

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
            let resolvedTitle = SmartFilenameResolver.bestTitle(metadata: metadata, sourceURL: sourceURL)
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
                log("Named \(target.lastPathComponent)")
                reservedFiles.insert(target.standardizedFileURL)
                renamedFiles.append(target.standardizedFileURL)
            } catch {
                log("Couldn't rename \(standardizedFile.lastPathComponent): \(error.localizedDescription)")
                reservedFiles.insert(standardizedFile)
                renamedFiles.append(standardizedFile)
            }
        }

        return renamedFiles.sorted { $0.path < $1.path }
    }

    private static func matchingMetadataIndex(
        for file: URL,
        in metadataRecords: [SmartFilenameMetadata],
        singleFileFallback: Bool
    ) -> Int? {
        if let exactIndex = metadataRecords.firstIndex(where: { metadata in
            guard let filepath = metadata.filepath else { return false }
            return URL(fileURLWithPath: filepath).standardizedFileURL == file
        }) {
            return exactIndex
        }

        let stemKey = normalizedKey(file.deletingPathExtension().lastPathComponent)
        if let identifierIndex = metadataRecords.firstIndex(where: { metadata in
            let idKey = normalizedKey(metadata.id)
            return !idKey.isEmpty && stemKey.contains(idKey)
        }) {
            return identifierIndex
        }

        return singleFileFallback ? metadataRecords.indices.first : nil
    }

    private static func uniqueURL(for desired: URL, reservedFiles: Set<URL>) -> URL {
        let fileManager = FileManager.default

        func isAvailable(_ url: URL) -> Bool {
            !reservedFiles.contains(url.standardizedFileURL) && !fileManager.fileExists(atPath: url.path)
        }

        if isAvailable(desired) { return desired }

        let folder = desired.deletingLastPathComponent()
        let stem = desired.deletingPathExtension().lastPathComponent
        let fileExtension = desired.pathExtension
        var index = 2

        while true {
            let filename = fileExtension.isEmpty ? "\(stem) \(index)" : "\(stem) \(index).\(fileExtension)"
            let candidate = folder.appendingPathComponent(filename)
            if isAvailable(candidate) { return candidate }
            index += 1
        }
    }

    private static func safeFileNameComponent(_ value: String) -> String {
        let forbidden = CharacterSet(charactersIn: "/:").union(.controlCharacters).union(.newlines)
        let sanitized = value.unicodeScalars.map { forbidden.contains($0) ? " " : String($0) }.joined()
        let collapsed = sanitized
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "."))
        return collapsed == "." || collapsed == ".." ? "" : collapsed
    }

    private static func limitedFileNameComponent(_ value: String, byteLimit: Int) -> String {
        var result = safeFileNameComponent(value)
        while result.utf8.count > byteLimit && !result.isEmpty {
            result.removeLast()
        }
        return result.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func normalizedKey(_ value: String?) -> String {
        guard let value else { return "" }
        return String(value.lowercased().unicodeScalars.filter { CharacterSet.alphanumerics.contains($0) })
    }

    // MARK: ffmpeg polish

    /// Rewrites MP4s with the index at the front, so they start playing before they finish loading.
    static func polish(
        files: [URL],
        ffmpegPath: String,
        environment: [String: String],
        process: ToolProcess,
        emit: @escaping (JobEvent) -> Void
    ) async throws -> Int {
        var count = 0
        let candidates = files.filter { ["mp4", "m4v", "mov"].contains($0.pathExtension.lowercased()) }

        for (index, file) in candidates.enumerated() {
            emit(.phase(.polishing, detail: candidates.count > 1
                ? "faststart, file \(index + 1) of \(candidates.count)"
                : "moving the index to the front"))
            let temp = file.deletingLastPathComponent()
                .appendingPathComponent(".\(file.deletingPathExtension().lastPathComponent).dawnload-polish.\(file.pathExtension)")

            do {
                let result = try await process.run(
                    executable: ffmpegPath,
                    arguments: ["-y", "-loglevel", "error", "-i", file.path, "-map", "0", "-c", "copy", "-movflags", "+faststart", temp.path],
                    environment: environment,
                    currentDirectory: file.deletingLastPathComponent(),
                    onLine: { emit(.log($0)) }
                )
                guard result.exitCode == 0 else {
                    try? FileManager.default.removeItem(at: temp)
                    emit(.log("Skipped faststart for \(file.lastPathComponent) (ffmpeg exit \(result.exitCode))"))
                    continue
                }
                try replace(file: file, with: temp)
                count += 1
            } catch let stopped as JobStopped {
                try? FileManager.default.removeItem(at: temp)
                throw stopped
            } catch {
                try? FileManager.default.removeItem(at: temp)
                emit(.log("Skipped faststart for \(file.lastPathComponent): \(error.localizedDescription)"))
            }
        }

        return count
    }

    private static func replace(file: URL, with replacement: URL) throws {
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
}
