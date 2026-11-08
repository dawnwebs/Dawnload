import Foundation

/// Saves the list to ~/Library/Application Support/Dawnload/items.json.
struct ItemStore {
    let fileURL: URL

    init(fileURL: URL? = nil) {
        if let fileURL {
            self.fileURL = fileURL
        } else {
            let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
                ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support")
            self.fileURL = support.appendingPathComponent("Dawnload/items.json")
        }
    }

    func load() -> [DownloadItem.Record] {
        guard let data = try? Data(contentsOf: fileURL) else { return [] }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return (try? decoder.decode(Envelope.self, from: data))?.items ?? []
    }

    func save(_ records: [DownloadItem.Record]) {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(Envelope(version: 1, items: records)) else { return }
        try? FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: fileURL, options: .atomic)
    }

    private struct Envelope: Codable {
        var version: Int
        var items: [DownloadItem.Record]
    }
}
