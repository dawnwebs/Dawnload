import AppKit
import Combine
import DawnloadCore
import SwiftUI

/// The app's state and every intent. The list is one ordered queue; engines run items lane by lane.
@MainActor
final class AppModel: ObservableObject {
    // The list
    @Published private(set) var items: [DownloadItem] = []
    @Published var selectedID: UUID?
    @Published var filter: ListFilter = .all
    /// Bumps whenever an item changes status, so counts and sections redraw.
    @Published private(set) var revision = 0
    /// The queue runs while this is on; it turns off when nothing is left to start.
    @Published private(set) var queueRunning = false

    // Intake
    @Published var draftText = "" {
        didSet { if intakeFailure != nil, draftText != oldValue { intakeFailure = nil } }
    }
    /// The chips. Sticky between launches; each item takes a copy.
    @Published var draft: DownloadOptions {
        didSet { saveDraft() }
    }
    @Published var intakeFailure: DownloadFailure?
    @Published var focusIntakeTrigger = 0

    // Settings and environment
    @Published var cookiesFile: URL? {
        didSet {
            UserDefaults.standard.set(cookiesFile?.path, forKey: Keys.cookies)
            draft.cookiesFile = cookiesFile
        }
    }
    @Published var appearance: AppearancePreference {
        didSet { UserDefaults.standard.set(appearance.rawValue, forKey: Keys.appearance) }
    }
    @Published private(set) var tools: [ToolStatus] = []
    @Published var showInspector = true
    @Published var showFirstRun = false

    let torrentsEnabled: Bool
    let isDemo: Bool

    private var snapshot: ToolSnapshot
    private var jobs: [UUID: DownloadJob] = [:]
    private var lanes: [UUID: EngineLane] = [:]
    private var removingAfterStop = Set<UUID>()
    private var saveTask: Task<Void, Never>?
    private let store = ItemStore(fileURL: UserDefaults.standard.string(forKey: "DawnloadStore").map { URL(fileURLWithPath: $0) })
    private let resolver = ToolResolver()

    private enum Keys {
        static let draft = "draftOptions"
        static let cookies = "cookiesFilePath"
        static let appearance = "appearance"
        static let firstRun = "hasCompletedFirstRun"
        static let legacyDestination = "defaultDestinationPath"
        static let legacyImpersonation = "impersonationTarget"
    }

    init() {
        let defaults = UserDefaults.standard
        isDemo = defaults.bool(forKey: "DawnloadDemo")
        torrentsEnabled = defaults.bool(forKey: "DawnloadTorrents")
        appearance = AppearancePreference(rawValue: defaults.string(forKey: Keys.appearance) ?? "") ?? .system

        var draft = DownloadOptions()
        if let data = defaults.data(forKey: Keys.draft),
           let saved = try? JSONDecoder().decode(DownloadOptions.self, from: data) {
            draft = saved
        } else {
            // Carry over the old app's settings.
            if let path = defaults.string(forKey: Keys.legacyDestination), !path.isEmpty {
                draft.destination = URL(fileURLWithPath: path, isDirectory: true)
            }
            if let raw = defaults.string(forKey: Keys.legacyImpersonation),
               let target = BrowserImpersonationTarget(rawValue: raw) {
                draft.impersonationTarget = target
            }
        }
        var cookies: URL?
        if let path = defaults.string(forKey: Keys.cookies), FileManager.default.isReadableFile(atPath: path) {
            cookies = URL(fileURLWithPath: path)
        }
        draft.cookiesFile = cookies
        // For test runs: download into another folder without changing the saved one.
        if let folder = defaults.string(forKey: "DawnloadFolder") {
            draft.destination = URL(fileURLWithPath: folder, isDirectory: true)
        }
        self.draft = draft
        self.cookiesFile = cookies
        snapshot = resolver.snapshot()

        if isDemo {
            // Review switches: -DawnloadEmpty, -DawnloadSelect <state>, -DawnloadFilter <filter>,
            // -DawnloadMissingTools, -DawnloadDraft <text>.
            if !defaults.bool(forKey: "DawnloadEmpty") {
                items = DemoData.items(destination: draft.destination, torrents: torrentsEnabled)
            }
            let select = ItemStatus(rawValue: defaults.string(forKey: "DawnloadSelect") ?? "pulling") ?? .pulling
            selectedID = items.first { $0.status == select }?.id
            filter = ListFilter(rawValue: defaults.string(forKey: "DawnloadFilter") ?? "") ?? .all
            if defaults.bool(forKey: "DawnloadMissingTools") {
                snapshot.paths[.ytDlp] = nil
                snapshot.paths[.ffmpeg] = nil
            }
            draftText = defaults.string(forKey: "DawnloadDraft") ?? ""
        } else {
            items = store.load().map(DownloadItem.init(record:))
        }

        showFirstRun = defaults.bool(forKey: "DawnloadFirstRun") || (!isDemo && !defaults.bool(forKey: Keys.firstRun))
        refreshTools()
        updateDockBadge()
    }

    // MARK: - Derived

    var selectedItem: DownloadItem? {
        guard let selectedID else { return nil }
        return items.first { $0.id == selectedID }
    }

    var visibleFilters: [ListFilter] {
        ListFilter.allCases.filter { $0 != .seeding || torrentsEnabled }
    }

    func count(_ filter: ListFilter) -> Int {
        _ = revision
        return items.filter { filter.includes($0.status) }.count
    }

    /// The list for the current filter, grouped and ordered.
    func sections() -> [(ListSection, [DownloadItem])] {
        _ = revision
        let visible = items.filter { filter.includes($0.status) }
        return ListSection.allCases.compactMap { section in
            var group = visible.filter { ListSection.of($0.status) == section }
            if section == .done {
                group.sort { ($0.finishedAt ?? $0.createdAt) > ($1.finishedAt ?? $1.createdAt) }
            }
            return group.isEmpty ? nil : (section, group)
        }
    }

    /// 1-based place among waiting items, for "Up next" and "3rd in line".
    func queuePosition(of item: DownloadItem) -> Int? {
        _ = revision
        let waiting = items.filter { $0.status == .queued }
        return waiting.firstIndex { $0.id == item.id }.map { $0 + 1 }
    }

    var hasActive: Bool { _ = revision; return items.contains { $0.status.isActive } }
    var hasWaiting: Bool { _ = revision; return items.contains { $0.status == .queued } }
    var hasPaused: Bool { _ = revision; return items.contains { $0.status == .paused } }

    var missingTools: [ToolID] { tools.filter { !$0.isInstalled }.map(\.tool) }
    var ytDlpMissing: Bool { snapshot.path(.ytDlp) == nil }

    var detectedSource: DownloadSource? { DownloadSource.parse(draftText) }

    var resolvedEngine: EngineID {
        detectedSource.map(draft.engine.resolve) ?? (draft.engine == .galleryDl ? .galleryDl : .ytDlp)
    }

    /// The detection line under the paste field.
    var detection: Detection? {
        if let intakeFailure {
            return Detection(text: intakeFailure.sentence, tone: .failed)
        }
        let text = draftText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return nil }
        guard let source = detectedSource else {
            return Detection(text: "A full link starts with https://.", tone: .neutral)
        }

        let engineID = draft.engine.resolve(for: source)
        let engine = EngineRegistry.engine(engineID)
        switch source {
        case .web:
            if let failure = engine.availability(tools: snapshot, options: draft).failure {
                return Detection(text: "\(source.shortName) · \(failure.sentence)", tone: .warning)
            }
            let auto = draft.engine == .auto ? " (Auto)" : ""
            return Detection(text: "\(source.shortName) · \(engine.displayName) will download this\(auto)", tone: .neutral)
        case .magnet(let magnet):
            let name = magnet.displayName.map { " · \($0)" } ?? ""
            return Detection(text: "Magnet link\(name) · Torrent downloads aren't built yet.", tone: .warning, kind: .magnet)
        case .torrentFile(let url):
            return Detection(text: ".torrent file · \(url.lastPathComponent) · Torrent downloads aren't built yet.", tone: .warning, kind: .torrentFile)
        }
    }

    /// Download is on when there's text, unless what was pasted can't be downloaded here yet
    /// (a missing tool, or a torrent with no engine). The detection line says why.
    var canSubmit: Bool {
        guard !draftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
        guard let source = detectedSource else { return true }
        let engine = EngineRegistry.engine(draft.engine.resolve(for: source))
        return engine.availability(tools: snapshot, options: draft) == .ready
    }

    // MARK: - Intake

    /// Links passed on the command line (`Dawnload https://…`) are added and downloaded.
    func downloadLaunchArguments() {
        guard !isDemo else { return }
        var arguments = Array(CommandLine.arguments.dropFirst())
        var links: [String] = []
        while let argument = arguments.first {
            arguments.removeFirst()
            if argument.hasPrefix("-") {
                // -Key value pairs are UserDefaults; skip the value.
                if !arguments.isEmpty { arguments.removeFirst() }
                continue
            }
            if DownloadSource.parse(argument) != nil { links.append(argument) }
        }
        for link in links {
            draftText = link
            download()
        }
    }

    /// Return / ⌘D: add the link and start the queue.
    func download() {
        guard let item = makeItemFromDraft() else { return }
        insert(item)
        queueRunning = true
        pump()
    }

    /// ⌥Return / ⇧⌘A: add the link and leave it waiting.
    func addToQueue() {
        guard let item = makeItemFromDraft() else { return }
        insert(item)
        if queueRunning { pump() }
    }

    /// ⇧⌘V.
    func pasteAndDownload() {
        guard let text = NSPasteboard.general.string(forType: .string)?
            .trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else {
            return
        }
        draftText = text
        download()
    }

    func pasteIntoField() {
        guard let text = NSPasteboard.general.string(forType: .string)?
            .trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else {
            return
        }
        draftText = text
        focusIntakeTrigger += 1
    }

    /// Drops: links, text, .torrent files.
    func accept(dropped text: String) {
        draftText = text.trimmingCharacters(in: .whitespacesAndNewlines)
        focusIntakeTrigger += 1
    }

    private func makeItemFromDraft() -> DownloadItem? {
        guard let source = detectedSource else {
            intakeFailure = .notALink
            return nil
        }
        let engine = EngineRegistry.engine(draft.engine.resolve(for: source))
        if let failure = engine.availability(tools: refreshSnapshot(), options: draft).failure {
            intakeFailure = failure
            return nil
        }
        var options = draft
        options.cookiesFile = cookiesFile
        let item = DownloadItem(source: source, options: options)
        draftText = ""
        intakeFailure = nil
        return item
    }

    private func insert(_ item: DownloadItem) {
        withAnimation(DLMotion.settle) {
            items.append(item)
            selectedID = item.id
            if !filter.includes(.queued) { filter = .all }
        }
        changed()
    }

    // MARK: - Queue

    func startQueue() {
        queueRunning = true
        pump()
    }

    func pauseAll() {
        queueRunning = false
        for item in items where jobs[item.id] != nil {
            pause(item)
        }
    }

    func resumeAll() {
        for item in items where item.status == .paused {
            item.status = .queued
        }
        changed()
        queueRunning = true
        pump()
    }

    func pause(_ item: DownloadItem) {
        guard let job = jobs[item.id] else { return }
        item.phaseDetail = "Pausing"
        job.stop()
    }

    /// Resume a paused item, retry a failed one, or start a waiting one now: it goes to the head
    /// of the waiting line and the queue runs.
    func runSoon(_ item: DownloadItem) {
        guard item.status.isEditable else { return }
        item.status = .queued
        item.failure = nil
        moveToHeadOfQueue(item)
        changed()
        queueRunning = true
        pump()
    }

    func remove(_ ids: Set<UUID>) {
        for id in ids {
            guard let item = items.first(where: { $0.id == id }) else { continue }
            if let job = jobs[id] {
                removingAfterStop.insert(id)
                job.stop()
                item.phaseDetail = "Stopping"
            } else {
                items.removeAll { $0.id == id }
            }
        }
        if let selectedID, ids.contains(selectedID) { self.selectedID = nil }
        changed()
    }

    func stopSeeding(_ item: DownloadItem) {
        guard item.status == .seeding else { return }
        jobs[item.id]?.stop()
    }

    func clear(_ status: ItemStatus) {
        remove(Set(items.filter { $0.status == status }.map(\.id)))
    }

    /// Reorders the waiting line. Offsets are positions among queued items.
    func moveWaiting(fromOffsets source: IndexSet, toOffset destination: Int) {
        var waiting = items.filter { $0.status == .queued }
        waiting.move(fromOffsets: source, toOffset: destination)
        items = items.filter { $0.status != .queued } + waiting
        changed()
    }

    func moveToTop(_ item: DownloadItem) {
        guard item.status == .queued else { return }
        moveToHeadOfQueue(item)
        changed()
    }

    private func moveToHeadOfQueue(_ item: DownloadItem) {
        items.removeAll { $0.id == item.id }
        let index = items.firstIndex { $0.status == .queued } ?? items.endIndex
        items.insert(item, at: index)
    }

    /// Starts what can start, lane by lane.
    private func pump() {
        guard queueRunning, !isDemo else { return }

        for lane in [EngineLane.web, .torrent] {
            var free = lane.maxConcurrent - lanes.values.filter { $0 == lane }.count
            for item in items where free > 0 && item.status == .queued
                && EngineRegistry.engine(item.engine).lane == lane {
                start(item)
                free -= 1
            }
        }

        if jobs.isEmpty, !items.contains(where: { $0.status == .queued }) {
            queueRunning = false
        }
    }

    private func start(_ item: DownloadItem) {
        let engine = EngineRegistry.engine(item.engine)
        let tools = refreshSnapshot()
        var options = item.options
        if options.useCookies, options.cookiesFile == nil { options.cookiesFile = cookiesFile }

        let job: DownloadJob
        do {
            if let failure = engine.availability(tools: tools, options: options).failure { throw failure }
            job = try engine.makeJob(for: JobRequest(
                itemID: item.id, source: item.source, options: options, tools: tools,
                selectedFiles: item.files.isEmpty ? nil : item.keptFiles.map(\.path)
            ))
        } catch {
            fail(item, error)
            return
        }

        item.resetForRun()
        item.status = .pulling
        item.appendLog("Started \(Date().formatted(date: .abbreviated, time: .standard))")
        jobs[item.id] = job
        lanes[item.id] = engine.lane
        changed()

        Task { [weak self] in
            do {
                let outcome = try await job.run { event in
                    DispatchQueue.main.async { self?.apply(event, to: item) }
                }
                DispatchQueue.main.async { self?.finish(item, outcome) }
            } catch {
                DispatchQueue.main.async { self?.ended(item, error: error) }
            }
        }
    }

    private func apply(_ event: JobEvent, to item: DownloadItem) {
        guard jobs[item.id] != nil else { return }
        switch event {
        case .command(let command):
            item.command = command
            item.appendLog("$ \(command)")
        case .log(let line):
            item.appendLog(line)
        case .phase(let status, let detail):
            if item.status != status {
                withAnimation(DLMotion.swap) { item.status = status }
                changed(save: false)
            }
            if item.phaseDetail != detail { item.phaseDetail = detail }
        case .transfer(let sample):
            withAnimation(DLMotion.fill) { item.transfer = TransferStats(sample) }
        case .title(let title):
            item.title = title
        case .files(let files):
            item.files = files
        case .swarm(let swarm):
            item.swarm = swarm
        }
    }

    private func finish(_ item: DownloadItem, _ outcome: JobOutcome) {
        jobs[item.id] = nil
        lanes[item.id] = nil
        if removingAfterStop.remove(item.id) != nil {
            items.removeAll { $0.id == item.id }
            changed()
            pump()
            return
        }

        if !outcome.files.isEmpty {
            item.files = FileFinishing.itemFiles(outcome.files)
        }
        var transfer = item.transfer
        transfer.fraction = 1
        transfer.bytesPerSecond = nil
        transfer.secondsLeft = nil
        let sizes = item.files.compactMap(\.size)
        if !sizes.isEmpty {
            transfer.bytesTotal = sizes.reduce(0, +)
            transfer.bytesDone = transfer.bytesTotal
        }
        item.transfer = transfer
        item.phaseDetail = nil
        withAnimation(DLMotion.swap) {
            item.status = outcome.continuesSeeding ? .seeding : .kept
        }
        item.finishedAt = Date()
        let polished = outcome.polishedCount > 0 ? ", \(outcome.polishedCount) polished" : ""
        item.appendLog("Kept \(DLFormat.count(item.files.count, "file")) in \(item.options.destination.path)\(polished).")
        changed()
        pump()
    }

    private func ended(_ item: DownloadItem, error: Error) {
        jobs[item.id] = nil
        lanes[item.id] = nil
        if removingAfterStop.remove(item.id) != nil {
            items.removeAll { $0.id == item.id }
            changed()
            pump()
            return
        }
        if error is JobStopped {
            item.phaseDetail = nil
            item.transfer.bytesPerSecond = nil
            item.transfer.secondsLeft = nil
            withAnimation(DLMotion.swap) {
                item.status = item.status == .seeding ? .kept : .paused
            }
            item.appendLog(item.status == .kept ? "Stopped seeding." : "Paused.")
            changed()
            pump()
            return
        }
        fail(item, error)
    }

    private func fail(_ item: DownloadItem, _ error: Error) {
        jobs[item.id] = nil
        lanes[item.id] = nil
        let failure = (error as? DownloadFailure)
            ?? DownloadFailure(kind: .other, message: error.localizedDescription, hint: "The log has the details.")
        item.failure = failure
        item.phaseDetail = nil
        item.transfer.bytesPerSecond = nil
        item.transfer.secondsLeft = nil
        withAnimation(DLMotion.swap) { item.status = .failed }
        item.finishedAt = Date()
        item.appendLog("Failed: \(failure.sentence)")
        if let detail = failure.detail { item.appendLog(detail) }
        changed()
        pump()
    }

    // MARK: - Items

    func reveal(_ item: DownloadItem) {
        let existing = item.keptFiles.map(\.url).filter { FileManager.default.fileExists(atPath: $0.path) }
        if existing.isEmpty {
            NSWorkspace.shared.open(item.options.destination)
        } else {
            NSWorkspace.shared.activateFileViewerSelecting(existing)
        }
    }

    func open(_ item: DownloadItem) {
        if let file = item.keptFiles.first(where: { FileManager.default.fileExists(atPath: $0.path) }) {
            NSWorkspace.shared.open(file.url)
        }
    }

    func copyLink(_ item: DownloadItem) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(item.source.absoluteString, forType: .string)
    }

    func copyLog(_ item: DownloadItem) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(item.log.joined(separator: "\n"), forType: .string)
    }

    /// Edits a waiting, paused or failed item's options.
    func update(_ item: DownloadItem, _ change: (inout DownloadOptions) -> Void) {
        guard item.status.isEditable else { return }
        var options = item.options
        change(&options)
        item.options = options
        changed()
    }

    func toggleFile(_ file: ItemFile, of item: DownloadItem) {
        guard item.status.isEditable, let index = item.files.firstIndex(of: file) else { return }
        item.files[index].isSelected.toggle()
        changed()
    }

    /// The one action that fits the item's state, for Return, ⌘Return and the inspector.
    func primaryAction(for item: DownloadItem) {
        switch item.status {
        case .kept: open(item)
        case .seeding: stopSeeding(item)
        case .pulling, .polishing: pause(item)
        case .queued, .paused, .failed: runSoon(item)
        }
    }

    // MARK: - Settings

    func chooseDestination() {
        let panel = NSOpenPanel()
        panel.title = "Choose Download Folder"
        panel.prompt = "Choose"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = draft.destination
        if panel.runModal() == .OK, let url = panel.url {
            draft.destination = url
        }
    }

    func revealDestination() {
        try? FileManager.default.createDirectory(at: draft.destination, withIntermediateDirectories: true)
        NSWorkspace.shared.open(draft.destination)
    }

    func chooseCookiesFile() {
        let panel = NSOpenPanel()
        panel.title = "Choose cookies.txt"
        panel.message = "Choose a cookies.txt exported in Netscape format from a browser session you own."
        panel.prompt = "Use This File"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            cookiesFile = url
            draft.useCookies = true
        }
    }

    func clearCookiesFile() {
        cookiesFile = nil
    }

    func openTorrentFile() {
        let panel = NSOpenPanel()
        panel.title = "Open Torrent File"
        panel.allowedContentTypes = [.init(filenameExtension: "torrent") ?? .data]
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            accept(dropped: url.path)
        }
    }

    func refreshTools() {
        if !(isDemo && UserDefaults.standard.bool(forKey: "DawnloadMissingTools")) {
            snapshot = resolver.snapshot()
        }
        tools = ToolID.allCases.map { ToolStatus(tool: $0, path: snapshot.path($0)) }
        let resolver = resolver
        for status in tools {
            guard let path = status.path else { continue }
            Task { [weak self] in
                let version = await resolver.version(of: status.tool, at: path)
                guard let self, let index = self.tools.firstIndex(where: { $0.tool == status.tool }) else { return }
                self.tools[index].version = version
            }
        }
    }

    @discardableResult
    private func refreshSnapshot() -> ToolSnapshot {
        if !isDemo { snapshot = resolver.snapshot() }
        return snapshot
    }

    func completeFirstRun() {
        UserDefaults.standard.set(true, forKey: Keys.firstRun)
        showFirstRun = false
        focusIntakeTrigger += 1
    }

    // MARK: - Bookkeeping

    private func changed(save: Bool = true) {
        revision &+= 1
        updateDockBadge()
        if save { scheduleSave() }
    }

    private func scheduleSave() {
        guard !isDemo else { return }
        saveTask?.cancel()
        saveTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 800_000_000)
            guard !Task.isCancelled else { return }
            self?.saveNow()
        }
    }

    private func saveNow() {
        guard !isDemo else { return }
        store.save(items.map(\.record))
    }

    private func saveDraft() {
        guard let data = try? JSONEncoder().encode(draft) else { return }
        UserDefaults.standard.set(data, forKey: Keys.draft)
    }

    private func updateDockBadge() {
        let pending = items.filter { $0.status.isActive || $0.status == .queued }.count
        NSApp?.dockTile.badgeLabel = pending > 0 ? "\(pending)" : nil
    }

    /// Called by the app delegate: stop the tools (partial files stay for Resume) and save.
    func prepareForQuit() {
        for job in jobs.values { job.stop() }
        saveNow()
    }
}

struct Detection: Equatable {
    enum Tone { case neutral, warning, failed }

    var text: String
    var tone: Tone
    var kind: DownloadSource.Kind = .web
}
