import AppKit
import DawnloadCore
import Foundation

@MainActor
final class DownloadViewModel: ObservableObject {
    @Published var urlText = ""
    @Published var selectedMode: DownloadMode = .ytDlp
    @Published var selectedQuality: QualityPreference = .best
    @Published var destination: URL
    @Published var useFFmpeg = true
    @Published var useBrowserImpersonation = false
    @Published var impersonationTarget: BrowserImpersonationTarget = .chrome146
    @Published var cookiesFile: URL?
    @Published var useYesPlaylist = false
    @Published var useGlassBackground = false {
        didSet {
            UserDefaults.standard.set(useGlassBackground, forKey: Self.glassBackgroundDefaultsKey)
        }
    }
    @Published var tools: [ToolStatus] = []
    @Published var state: RunState = .idle
    @Published var progress: Double?
    @Published var logLines: [String] = []
    @Published var commandPreview = ""
    @Published var queue: [DownloadQueueItem] = []
    @Published var activeQueueItemID: UUID?

    private let resolver = ToolResolver()
    private var runner: DownloadRunner?
    private var task: Task<Void, Never>?
    private static let destinationDefaultsKey = "defaultDestinationPath"
    private static let glassBackgroundDefaultsKey = "useGlassBackground"
    private static let cookiesFileDefaultsKey = "cookiesFilePath"
    private static let impersonationTargetDefaultsKey = "impersonationTarget"

    init() {
        let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
        let defaultDestination = (downloads ?? URL(fileURLWithPath: NSHomeDirectory()))
            .appendingPathComponent("Dawnload", isDirectory: true)

        if let savedPath = UserDefaults.standard.string(forKey: Self.destinationDefaultsKey),
           !savedPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            destination = URL(fileURLWithPath: savedPath, isDirectory: true)
        } else {
            destination = defaultDestination
        }

        useGlassBackground = UserDefaults.standard.bool(forKey: Self.glassBackgroundDefaultsKey)

        if let rawTarget = UserDefaults.standard.string(forKey: Self.impersonationTargetDefaultsKey),
           let savedTarget = BrowserImpersonationTarget(rawValue: rawTarget) {
            impersonationTarget = savedTarget
        }

        if let savedCookiesPath = UserDefaults.standard.string(forKey: Self.cookiesFileDefaultsKey),
           FileManager.default.isReadableFile(atPath: savedCookiesPath) {
            cookiesFile = URL(fileURLWithPath: savedCookiesPath)
        }

        refreshTools()
    }

    var canStart: Bool {
        !state.isRunning && !trimmedURL.isEmpty
    }

    var canAddToQueue: Bool {
        !state.isRunning && !trimmedURL.isEmpty
    }

    var canStartQueue: Bool {
        !state.isRunning && !queue.isEmpty
    }

    var primaryTool: ToolStatus? {
        tools.first { $0.name == selectedMode.executableName }
    }

    var ffmpegTool: ToolStatus? {
        tools.first { $0.name == "ffmpeg" }
    }

    var statusDetail: String {
        switch state {
        case .idle:
            if queue.isEmpty {
                "Choose a mode, paste a URL, and download into \(destination.lastPathComponent)."
            } else {
                "\(queue.count) queued item\(queue.count == 1 ? "" : "s") ready."
            }
        case .queued(let message):
            message
        case .running:
            if let progress {
                "\(Int(progress * 100))% complete"
            } else {
                "Working..."
            }
        case .completed(let message):
            message
        case .failed(let message):
            message
        case .cancelled:
            "The active download was stopped."
        }
    }

    func refreshTools() {
        tools = ["yt-dlp", "gallery-dl", "ffmpeg"].map {
            ToolStatus(name: $0, path: resolver.path(for: $0))
        }
    }

    func chooseDestination() {
        let panel = NSOpenPanel()
        panel.title = "Choose Download Folder"
        panel.prompt = "Choose"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = destination

        if panel.runModal() == .OK, let url = panel.url {
            destination = url
            UserDefaults.standard.set(url.path, forKey: Self.destinationDefaultsKey)
        }
    }

    func chooseCookiesFile() {
        let panel = NSOpenPanel()
        panel.title = "Choose Netscape Cookies File"
        panel.message = "Select a cookies.txt file exported from a browser session you own."
        panel.prompt = "Use Cookies"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false

        if panel.runModal() == .OK, let url = panel.url {
            cookiesFile = url
            UserDefaults.standard.set(url.path, forKey: Self.cookiesFileDefaultsKey)
        }
    }

    func clearCookiesFile() {
        cookiesFile = nil
        UserDefaults.standard.removeObject(forKey: Self.cookiesFileDefaultsKey)
    }

    func selectImpersonationTarget(_ target: BrowserImpersonationTarget) {
        impersonationTarget = target
        UserDefaults.standard.set(target.rawValue, forKey: Self.impersonationTargetDefaultsKey)
    }

    func startDownload() {
        guard !state.isRunning else { return }

        refreshTools()

        guard let url = normalizedURL(from: trimmedURL) else {
            state = .failed("Paste a complete URL, including https://.")
            return
        }
        urlText = url.absoluteString

        guard let request = makeRequest(
            url: url.absoluteString,
            mode: selectedMode,
            quality: selectedQuality,
            destination: destination,
            useFFmpeg: useFFmpeg,
            useBrowserImpersonation: useBrowserImpersonation,
            impersonationTarget: impersonationTarget,
            cookiesFile: cookiesFile,
            useYesPlaylist: useYesPlaylist
        ) else {
            return
        }

        run(entries: [QueueRunEntry(item: nil, request: request)])
    }

    @discardableResult
    func addCurrentToQueue() -> Bool {
        guard !state.isRunning else { return false }

        guard let url = normalizedURL(from: trimmedURL) else {
            state = .failed("Paste a complete URL, including https://.")
            return false
        }

        queue.append(
            DownloadQueueItem(
                url: url.absoluteString,
                mode: selectedMode,
                quality: selectedQuality,
                destination: destination,
                useFFmpeg: useFFmpeg,
                useBrowserImpersonation: useBrowserImpersonation,
                impersonationTarget: impersonationTarget,
                cookiesFile: cookiesFile,
                useYesPlaylist: useYesPlaylist
            )
        )
        urlText = ""
        progress = nil
        state = .queued("\(queue.count) queued item\(queue.count == 1 ? "" : "s") ready.")
        return true
    }

    func removeQueuedItem(_ item: DownloadQueueItem) {
        guard !state.isRunning else { return }
        queue.removeAll { $0.id == item.id }

        if queue.isEmpty, case .queued = state {
            state = .idle
        } else if case .queued = state {
            state = .queued("\(queue.count) queued item\(queue.count == 1 ? "" : "s") ready.")
        }
    }

    func clearQueue() {
        guard !state.isRunning else { return }
        queue.removeAll()
        activeQueueItemID = nil

        if case .queued = state {
            state = .idle
        }
    }

    func startQueue() {
        guard !state.isRunning else { return }
        guard !queue.isEmpty else {
            state = .failed("Add at least one URL to the queue first.")
            return
        }

        refreshTools()

        var entries: [QueueRunEntry] = []
        for item in queue {
            guard let request = makeRequest(
                url: item.url,
                mode: item.mode,
                quality: item.quality,
                destination: item.destination,
                useFFmpeg: item.useFFmpeg,
                useBrowserImpersonation: item.useBrowserImpersonation,
                impersonationTarget: item.impersonationTarget,
                cookiesFile: item.cookiesFile,
                useYesPlaylist: item.useYesPlaylist
            ) else {
                return
            }

            entries.append(QueueRunEntry(item: item, request: request))
        }

        run(entries: entries)
    }

    func cancelDownload() {
        task?.cancel()
        runner?.cancel()
    }

    func clearLog() {
        logLines.removeAll()
    }

    private func run(entries: [QueueRunEntry]) {
        guard !entries.isEmpty else { return }

        progress = nil
        logLines.removeAll()
        commandPreview = ""
        state = .running(entries.count > 1 ? "Queue 1 of \(entries.count)" : "Preparing")

        let total = entries.count

        task = Task { [weak self] in
            var completed = 0
            var lastSummary: DownloadSummary?

            do {
                for (index, entry) in entries.enumerated() {
                    if Task.isCancelled {
                        throw CancellationError()
                    }

                    let runner = DownloadRunner()

                    await MainActor.run { [weak self] in
                        self?.runner = runner
                        self?.activeQueueItemID = entry.item?.id
                        self?.progress = nil
                        self?.state = .running(total > 1 ? "Queue \(index + 1) of \(total)" : "Preparing")

                        if total > 1 {
                            self?.appendLog("Starting queued item \(index + 1) of \(total): \(entry.request.url)")
                        }
                    }

                    let summary = try await runner.run(entry.request) { event in
                        Task { @MainActor [weak self] in
                            self?.handle(event)
                        }
                    }

                    completed += 1
                    lastSummary = summary

                    await MainActor.run { [weak self] in
                        if let item = entry.item {
                            self?.queue.removeAll { $0.id == item.id }
                        }

                        self?.activeQueueItemID = nil
                        self?.runner = nil
                    }
                }

                await MainActor.run { [weak self] in
                    self?.progress = 1

                    if total > 1 {
                        self?.state = .completed("Queue complete. \(completed) item\(completed == 1 ? "" : "s") saved.")
                    } else if let request = entries.first?.request {
                        if request.ffmpegPath == nil {
                            self?.state = .completed("Saved to \(request.destination.path).")
                        } else {
                            let finalizedFiles = lastSummary?.finalizedFiles ?? 0
                            let suffix = finalizedFiles == 1 ? "1 file optimized with ffmpeg." : "\(finalizedFiles) files optimized with ffmpeg."
                            self?.state = .completed("Saved to \(request.destination.path). \(suffix)")
                        }
                    } else {
                        self?.state = .completed("Download complete.")
                    }

                    self?.activeQueueItemID = nil
                    self?.runner = nil
                    self?.task = nil
                }
            } catch is CancellationError {
                await MainActor.run { [weak self] in
                    self?.state = .cancelled
                    self?.activeQueueItemID = nil
                    self?.runner = nil
                    self?.task = nil
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.state = .failed(error.localizedDescription)
                    self?.activeQueueItemID = nil
                    self?.runner = nil
                    self?.task = nil
                }
            }
        }
    }

    private func handle(_ event: DownloadEvent) {
        switch event {
        case .command(let command):
            commandPreview = command
            appendLog("$ \(command)")
        case .line(let line):
            appendLog(line)
        case .phase(let phase):
            state = .running(phase)
            appendLog(phase)
        case .progress(let value):
            if let value {
                progress = value
            }
        }
    }

    private func appendLog(_ line: String) {
        logLines.append(line)

        if logLines.count > 500 {
            logLines.removeFirst(logLines.count - 500)
        }
    }

    private var trimmedURL: String {
        urlText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func normalizedURL(from text: String) -> URL? {
        normalizedDownloadURL(from: text)
    }

    private func makeRequest(
        url: String,
        mode: DownloadMode,
        quality: QualityPreference,
        destination: URL,
        useFFmpeg: Bool,
        useBrowserImpersonation: Bool,
        impersonationTarget: BrowserImpersonationTarget,
        cookiesFile: URL?,
        useYesPlaylist: Bool
    ) -> DownloadRequest? {
        guard resolver.path(for: mode.executableName) != nil else {
            state = .failed("\(mode.displayName) was not found. Install it with Homebrew, then refresh.")
            return nil
        }

        let ffmpegPath = useFFmpeg ? resolver.path(for: "ffmpeg") : nil
        if useFFmpeg && ffmpegPath == nil {
            state = .failed("ffmpeg was not found. Install it with Homebrew, then refresh.")
            return nil
        }

        if let cookiesFile,
           !FileManager.default.isReadableFile(atPath: cookiesFile.path) {
            state = .failed("The selected cookies file is no longer readable. Choose it again in Settings.")
            return nil
        }

        return DownloadRequest(
            url: url,
            mode: mode,
            quality: quality,
            destination: destination,
            ffmpegPath: ffmpegPath,
            useBrowserImpersonation: useBrowserImpersonation,
            impersonationTarget: impersonationTarget,
            cookiesFile: cookiesFile,
            useYesPlaylist: useYesPlaylist,
            environment: resolver.processEnvironment()
        )
    }
}

private struct QueueRunEntry {
    let item: DownloadQueueItem?
    let request: DownloadRequest
}
