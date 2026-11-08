import Foundation

/// Runs one command-line tool and hands back its output line by line. Shared by every engine.
final class ToolProcess {
    struct Result {
        let exitCode: Int32
        /// The last 64 KB of stdout and stderr together.
        let output: String
    }

    private let lock = NSLock()
    private var process: Process?
    private var stopped = false

    var wasStopped: Bool {
        lock.lock()
        defer { lock.unlock() }
        return stopped
    }

    /// Terminates the running tool. `run` then throws `JobStopped`.
    func stop() {
        lock.lock()
        stopped = true
        let running = process
        lock.unlock()
        if running?.isRunning == true {
            running?.interrupt()
            DispatchQueue.global().asyncAfter(deadline: .now() + 2) {
                if running?.isRunning == true { running?.terminate() }
            }
        }
    }

    /// Runs to completion. A non-zero exit is a result, not an error: the engine decides what it means.
    func run(
        executable: String,
        arguments: [String],
        environment: [String: String],
        currentDirectory: URL,
        onLine: @escaping (String) -> Void
    ) async throws -> Result {
        if wasStopped { throw JobStopped() }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.environment = environment
        process.currentDirectoryURL = currentDirectory

        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        process.standardInput = FileHandle.nullDevice

        let collector = OutputCollector()
        let outLines = LineBuffer(onLine: onLine, collector: collector)
        let errLines = LineBuffer(onLine: onLine, collector: collector)

        outputPipe.fileHandleForReading.readabilityHandler = { handle in
            outLines.append(handle.availableData)
        }
        errorPipe.fileHandleForReading.readabilityHandler = { handle in
            errLines.append(handle.availableData)
        }

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Result, Error>) in
                let once = Once()
                process.terminationHandler = { [weak self] finished in
                    outputPipe.fileHandleForReading.readabilityHandler = nil
                    errorPipe.fileHandleForReading.readabilityHandler = nil
                    outLines.append(outputPipe.fileHandleForReading.readDataToEndOfFile())
                    errLines.append(errorPipe.fileHandleForReading.readDataToEndOfFile())
                    outLines.flush()
                    errLines.flush()

                    once.run {
                        self?.clear(finished)
                        if self?.wasStopped == true, finished.terminationStatus != 0 {
                            continuation.resume(throwing: JobStopped())
                        } else {
                            continuation.resume(returning: Result(exitCode: finished.terminationStatus, output: collector.text))
                        }
                    }
                }

                do {
                    self.set(process)
                    try process.run()
                } catch {
                    self.clear(process)
                    outputPipe.fileHandleForReading.readabilityHandler = nil
                    errorPipe.fileHandleForReading.readabilityHandler = nil
                    once.run { continuation.resume(throwing: error) }
                }
            }
        } onCancel: {
            self.stop()
        }
    }

    private func set(_ process: Process) {
        lock.lock()
        self.process = process
        lock.unlock()
    }

    private func clear(_ process: Process) {
        lock.lock()
        if self.process === process { self.process = nil }
        lock.unlock()
    }
}

/// Splits a pipe's chunks into whole lines; yt-dlp's carriage-return progress counts as a line end.
private final class LineBuffer {
    private let lock = NSLock()
    private var pending = Data()
    private let onLine: (String) -> Void
    private let collector: OutputCollector

    init(onLine: @escaping (String) -> Void, collector: OutputCollector) {
        self.onLine = onLine
        self.collector = collector
    }

    func append(_ data: Data) {
        guard !data.isEmpty else { return }
        collector.append(data)
        lock.lock()
        pending.append(data)
        var lines: [String] = []
        while let index = pending.firstIndex(where: { $0 == 0x0A || $0 == 0x0D }) {
            let lineData = pending[pending.startIndex..<index]
            pending.removeSubrange(pending.startIndex...index)
            if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                lines.append(line)
            }
        }
        lock.unlock()
        lines.forEach(onLine)
    }

    func flush() {
        lock.lock()
        let rest = pending
        pending.removeAll()
        lock.unlock()
        if let line = String(data: rest, encoding: .utf8), !line.isEmpty {
            onLine(line)
        }
    }
}

private final class OutputCollector {
    private let lock = NSLock()
    private var data = Data()
    private let limit = 64 * 1024

    func append(_ newData: Data) {
        lock.lock()
        defer { lock.unlock() }
        data.append(newData)
        if data.count > limit {
            data.removeFirst(data.count - limit)
        }
    }

    var text: String {
        lock.lock()
        defer { lock.unlock() }
        return String(decoding: data, as: UTF8.self)
    }
}

private final class Once {
    private let lock = NSLock()
    private var done = false

    func run(_ block: () -> Void) {
        lock.lock()
        defer { lock.unlock() }
        guard !done else { return }
        done = true
        block()
    }
}

func shellPreview(executable: String, arguments: [String]) -> String {
    ([executable] + arguments)
        .map { value in
            if value.range(of: #"^[A-Za-z0-9_/\-.:=%+,@]+$"#, options: .regularExpression) != nil {
                return value
            }
            return "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
        }
        .joined(separator: " ")
}
