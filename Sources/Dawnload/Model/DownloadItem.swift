import DawnloadCore
import Foundation
import SwiftUI

/// Where an item is in its life. The words are the brand's state names.
enum ItemStatus: String, Codable, CaseIterable {
    case queued
    case pulling
    case polishing
    case kept
    case failed
    case paused
    /// Torrents only: kept on disk and still uploading.
    case seeding

    var word: String {
        switch self {
        case .queued: "Queued"
        case .pulling: "Pulling"
        case .polishing: "Polishing"
        case .kept: "Kept"
        case .failed: "Failed"
        case .paused: "Paused"
        case .seeding: "Seeding"
        }
    }

    var isActive: Bool { self == .pulling || self == .polishing }
    var isFinished: Bool { self == .kept || self == .failed || self == .seeding }
    /// Its options can still change, because nothing is running.
    var isEditable: Bool { self == .queued || self == .paused || self == .failed }
    /// The files are on disk.
    var hasFiles: Bool { self == .kept || self == .seeding }
}

/// One file an item made or will make. Torrents have many and can skip some.
struct ItemFile: Codable, Identifiable, Equatable {
    var id: String { path }
    /// Absolute path once known, or the path inside the torrent.
    var path: String
    var size: Int64?
    var bytesDone: Int64?
    /// Torrents: unticked files are skipped.
    var isSelected = true

    var name: String { (path as NSString).lastPathComponent }
    var url: URL { URL(fileURLWithPath: path) }
}

struct TransferStats: Codable, Equatable {
    var fraction: Double?
    var bytesDone: Int64?
    var bytesTotal: Int64?
    var bytesPerSecond: Double?
    var secondsLeft: Int?
    var entryIndex: Int?
    var entryCount: Int?

    init(_ sample: TransferSample) {
        fraction = sample.fraction
        bytesDone = sample.bytesDone
        bytesTotal = sample.bytesTotal
        bytesPerSecond = sample.bytesPerSecond
        secondsLeft = sample.secondsLeft
        entryIndex = sample.entryIndex
        entryCount = sample.entryCount
    }

    init(fraction: Double? = nil, bytesDone: Int64? = nil, bytesTotal: Int64? = nil, bytesPerSecond: Double? = nil,
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

/// Torrents: the people this item is trading pieces with.
struct SwarmStats: Codable, Equatable {
    var peers: Int
    var seeds: Int
    var downloadRate: Double
    var uploadRate: Double
    var uploaded: Int64
    var ratio: Double
}

@MainActor
final class DownloadItem: ObservableObject, Identifiable {
    let id: UUID
    let source: DownloadSource
    let createdAt: Date

    @Published var options: DownloadOptions {
        didSet { engine = options.engine.resolve(for: source) }
    }
    /// The engine that runs it, resolved from the options' preference for this source.
    @Published private(set) var engine: EngineID
    @Published var title: String?
    @Published var status: ItemStatus
    /// A few words on what the engine is doing: "merging audio and video", "Checking".
    @Published var phaseDetail: String?
    @Published var transfer = TransferStats()
    @Published var swarm: SwarmStats?
    @Published var files: [ItemFile] = []
    @Published var failure: DownloadFailure?
    @Published var log: [String] = []
    @Published var command: String?
    @Published var finishedAt: Date?

    init(id: UUID = UUID(), source: DownloadSource, options: DownloadOptions, status: ItemStatus = .queued, createdAt: Date = Date()) {
        self.id = id
        self.source = source
        self.options = options
        self.status = status
        self.createdAt = createdAt
        self.engine = options.engine.resolve(for: source)
    }

    var displayTitle: String {
        if let title, !title.isEmpty { return title }
        return source.provisionalTitle
    }

    var keptFiles: [ItemFile] { files.filter(\.isSelected) }

    var keptBytes: Int64? {
        let sizes = keptFiles.compactMap(\.size)
        return sizes.isEmpty ? transfer.bytesTotal : sizes.reduce(0, +)
    }

    /// The fill of the D: real progress, or where it stopped.
    var fill: Double {
        switch status {
        case .kept, .seeding, .polishing, .failed: 1
        case .queued: 0
        case .pulling, .paused: transfer.fraction ?? 0
        }
    }

    func appendLog(_ line: String) {
        log.append(line)
        if log.count > 400 { log.removeFirst(log.count - 400) }
    }

    func resetForRun() {
        failure = nil
        phaseDetail = nil
        swarm = nil
        finishedAt = nil
        if status != .paused { transfer = TransferStats() }
    }

    // MARK: Persistence

    struct Record: Codable {
        var id: UUID
        var source: DownloadSource
        var options: DownloadOptions
        var title: String?
        var status: ItemStatus
        var transfer: TransferStats
        var files: [ItemFile]
        var failure: DownloadFailure?
        var log: [String]
        var command: String?
        var createdAt: Date
        var finishedAt: Date?
    }

    var record: Record {
        Record(id: id, source: source, options: options, title: title, status: status, transfer: transfer,
               files: files, failure: failure, log: Array(log.suffix(120)), command: command,
               createdAt: createdAt, finishedAt: finishedAt)
    }

    convenience init(record: Record) {
        // Anything that was running when the app quit comes back paused.
        let status: ItemStatus = switch record.status {
        case .pulling, .polishing: .paused
        case .seeding: .kept
        default: record.status
        }
        self.init(id: record.id, source: record.source, options: record.options, status: status, createdAt: record.createdAt)
        title = record.title
        transfer = record.transfer
        transfer.bytesPerSecond = nil
        transfer.secondsLeft = nil
        files = record.files
        failure = record.failure
        log = record.log
        command = record.command
        finishedAt = record.finishedAt
    }
}
