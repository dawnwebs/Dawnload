import Foundation

/// Machine-readable lines Dawnload asks yt-dlp to print, so progress, parts and
/// polishing never depend on yt-dlp's human output.
public enum YTDlpMarkers {
    public static let item = "__DAWNLOAD_ITEM__"
    public static let parts = "__DAWNLOAD_PARTS__"
    public static let progress = "__DAWNLOAD_PROGRESS__"
    public static let post = "__DAWNLOAD_POST__"

    /// The yt-dlp arguments that make it print the lines `YTDlpLine.parse` reads.
    public static var arguments: [String] {
        [
            "--print", "before_dl:\(item)%(.{id,title,playlist_index,n_entries,format_id,filesize,filesize_approx})j",
            "--print", "before_dl:\(parts)%(requested_formats.:.{format_id,filesize,filesize_approx})j",
            "--progress-template",
            "download:\(progress)%(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s %(info.format_id)s %(progress.status)s",
            "--progress-template", "postprocess:\(post)%(progress.status)s %(progress.postprocessor)s"
        ]
    }

    public static let all = [item, parts, progress, post, YTDlpMetadataParser.marker]
}

public struct YTDlpItemInfo: Decodable, Equatable {
    public var id: String?
    public var title: String?
    public var playlistIndex: Int?
    public var entryCount: Int?
    public var formatID: String?
    public var filesize: Int64?
    public var filesizeApprox: Int64?

    public init(id: String? = nil, title: String? = nil, playlistIndex: Int? = nil, entryCount: Int? = nil,
                formatID: String? = nil, filesize: Int64? = nil, filesizeApprox: Int64? = nil) {
        self.id = id
        self.title = title
        self.playlistIndex = playlistIndex
        self.entryCount = entryCount
        self.formatID = formatID
        self.filesize = filesize
        self.filesizeApprox = filesizeApprox
    }

    private enum CodingKeys: String, CodingKey {
        case id, title
        case playlistIndex = "playlist_index"
        case entryCount = "n_entries"
        case formatID = "format_id"
        case filesize
        case filesizeApprox = "filesize_approx"
    }

    public var expectedSize: Int64? { filesize ?? filesizeApprox }
}

public struct YTDlpPart: Decodable, Equatable {
    public var formatID: String
    public var filesize: Int64?
    public var filesizeApprox: Int64?

    public init(formatID: String, filesize: Int64? = nil, filesizeApprox: Int64? = nil) {
        self.formatID = formatID
        self.filesize = filesize
        self.filesizeApprox = filesizeApprox
    }

    private enum CodingKeys: String, CodingKey {
        case formatID = "format_id"
        case filesize
        case filesizeApprox = "filesize_approx"
    }

    public var expectedSize: Int64? { filesize ?? filesizeApprox }
}

public struct YTDlpProgressLine: Equatable {
    public var downloaded: Int64?
    public var total: Int64?
    public var totalEstimate: Int64?
    public var speed: Double?
    public var eta: Int?
    public var formatID: String?
    public var finished: Bool

    public init(downloaded: Int64?, total: Int64?, totalEstimate: Int64?, speed: Double?, eta: Int?, formatID: String?, finished: Bool) {
        self.downloaded = downloaded
        self.total = total
        self.totalEstimate = totalEstimate
        self.speed = speed
        self.eta = eta
        self.formatID = formatID
        self.finished = finished
    }
}

public enum YTDlpLine: Equatable {
    case item(YTDlpItemInfo)
    case parts([YTDlpPart])
    case progress(YTDlpProgressLine)
    /// A postprocessor started or finished. `isPolish` is false for yt-dlp's own file moves.
    case postprocess(started: Bool, name: String, isPolish: Bool)
    case metadata
    case text(String)

    public static func parse(_ line: String) -> YTDlpLine {
        if let payload = payload(of: line, marker: YTDlpMarkers.progress) {
            let fields = payload.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
            guard fields.count >= 7 else { return .text(line) }
            return .progress(YTDlpProgressLine(
                downloaded: int64(fields[0]),
                total: int64(fields[1]),
                totalEstimate: int64(fields[2]),
                speed: Double(fields[3]).flatMap { $0.isFinite && $0 >= 0 ? $0 : nil },
                eta: Double(fields[4]).map { Int($0.rounded()) },
                formatID: fields[5] == "NA" ? nil : fields[5],
                finished: fields[6] == "finished"
            ))
        }

        if let payload = payload(of: line, marker: YTDlpMarkers.item) {
            guard let data = payload.data(using: .utf8),
                  let info = try? JSONDecoder().decode(YTDlpItemInfo.self, from: data) else {
                return .text(line)
            }
            return .item(info)
        }

        if let payload = payload(of: line, marker: YTDlpMarkers.parts) {
            guard let data = payload.data(using: .utf8),
                  let parts = try? JSONDecoder().decode([YTDlpPart].self, from: data) else {
                return .parts([])
            }
            return .parts(parts)
        }

        if let payload = payload(of: line, marker: YTDlpMarkers.post) {
            let fields = payload.split(separator: " ", maxSplits: 1).map(String.init)
            guard fields.count == 2 else { return .text(line) }
            let name = fields[1]
            return .postprocess(started: fields[0] == "started", name: name, isPolish: name != "MoveFiles")
        }

        if line.contains(YTDlpMetadataParser.marker) {
            return .metadata
        }

        return .text(line)
    }

    /// Words for a postprocessor, for "Polishing · merging audio and video".
    public static func polishDescription(for postprocessor: String) -> String {
        switch postprocessor {
        case "Merger": "merging audio and video"
        case "VideoRemuxer", "VideoConvertor": "remuxing to MP4"
        case "ExtractAudio": "extracting audio"
        case "Metadata", "FFmpegMetadata": "writing metadata"
        case "EmbedThumbnail": "embedding the thumbnail"
        case "FixupM3u8", "FixupM4a", "FixupStretched", "FixupDuplicateMoov", "FixupTimestamp", "FixupDuration":
            "fixing the container"
        default: "finishing with ffmpeg"
        }
    }

    private static func payload(of line: String, marker: String) -> String? {
        guard let range = line.range(of: marker) else { return nil }
        return String(line[range.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func int64(_ field: String) -> Int64? {
        if let value = Int64(field) { return value }
        if let value = Double(field), value.isFinite, value >= 0 { return Int64(value) }
        return nil
    }
}

/// Real progress for one item: bytes across its parts (video + audio), entries across a playlist.
public struct TransferSample: Equatable {
    /// 0…1 across the whole item, nil when the size isn't known.
    public var fraction: Double?
    public var bytesDone: Int64?
    public var bytesTotal: Int64?
    public var bytesPerSecond: Double?
    public var secondsLeft: Int?
    public var entryIndex: Int?
    public var entryCount: Int?

    public init(fraction: Double? = nil, bytesDone: Int64? = nil, bytesTotal: Int64? = nil, bytesPerSecond: Double? = nil,
                secondsLeft: Int? = nil, entryIndex: Int? = nil, entryCount: Int? = nil) {
        self.fraction = fraction
        self.bytesDone = bytesDone
        self.bytesTotal = bytesTotal
        self.bytesPerSecond = bytesPerSecond
        self.secondsLeft = secondsLeft
        self.entryIndex = entryIndex
        self.entryCount = entryCount
    }
}

public struct YTDlpProgressTracker {
    public private(set) var title: String?
    private var entryIndex: Int?
    private var entryCount: Int?
    private var entrySize: Int64?
    private var partOrder: [String] = []
    private var partSizes: [String: Int64] = [:]
    private var finishedBytes: [String: Int64] = [:]

    public init() {}

    /// Feeds one parsed line. Returns a sample when the line moved progress.
    public mutating func consume(_ line: YTDlpLine) -> TransferSample? {
        switch line {
        case .item(let info):
            title = info.title ?? title
            entryIndex = info.playlistIndex
            entryCount = info.entryCount
            entrySize = info.expectedSize
            partOrder = []
            partSizes = [:]
            finishedBytes = [:]
            return nil

        case .parts(let parts):
            partOrder = parts.map(\.formatID)
            for part in parts {
                if let size = part.expectedSize { partSizes[part.formatID] = size }
            }
            return nil

        case .progress(let progress):
            return sample(for: progress)

        default:
            return nil
        }
    }

    private mutating func sample(for progress: YTDlpProgressLine) -> TransferSample {
        let key = progress.formatID ?? "_"
        let currentTotal = progress.total ?? progress.totalEstimate ?? partSizes[key]
        if let currentTotal, partSizes[key] == nil || progress.total != nil {
            partSizes[key] = currentTotal
        }
        if !partOrder.isEmpty, !partOrder.contains(key) {
            partOrder.append(key)
        }

        let current = progress.downloaded ?? 0
        if progress.finished {
            finishedBytes[key] = max(current, currentTotal ?? current)
        }

        var done: Int64?
        var total: Int64?

        if partOrder.count > 1 {
            let sizes = partOrder.compactMap { partSizes[$0] }
            if sizes.count == partOrder.count {
                total = sizes.reduce(0, +)
                let finished = finishedBytes.filter { $0.key != key }.values.reduce(0, +)
                done = finished + (progress.finished ? finishedBytes[key] ?? current : current)
            }
        } else {
            total = currentTotal ?? entrySize
            done = progress.finished ? (finishedBytes[key] ?? current) : current
        }

        var entryFraction: Double?
        if let done, let total, total > 0 {
            entryFraction = min(max(Double(done) / Double(total), 0), 1)
        } else if partOrder.count > 1, let currentTotal, currentTotal > 0,
                  let index = partOrder.firstIndex(of: key) {
            // Sizes unknown for some parts: weight parts equally.
            let partFraction = min(Double(current) / Double(currentTotal), 1)
            entryFraction = (Double(index) + partFraction) / Double(partOrder.count)
        }

        var fraction = entryFraction
        if let entryIndex, let entryCount, entryCount > 1 {
            fraction = (Double(entryIndex - 1) + (entryFraction ?? 0)) / Double(entryCount)
        }

        var secondsLeft = progress.eta
        if let speed = progress.speed, speed > 0, let done, let total, total >= done {
            secondsLeft = Int((Double(total - done) / speed).rounded())
        }

        return TransferSample(
            fraction: fraction,
            bytesDone: done,
            bytesTotal: total,
            bytesPerSecond: progress.speed,
            secondsLeft: progress.finished ? nil : secondsLeft,
            entryIndex: entryIndex,
            entryCount: entryCount
        )
    }
}
