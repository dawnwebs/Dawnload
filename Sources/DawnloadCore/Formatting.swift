import Foundation

/// Numbers in the brand's voice: short, specific, unit last.
public enum DLFormat {
    public static func bytes(_ value: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        formatter.allowedUnits = [.useKB, .useMB, .useGB, .useTB]
        formatter.includesActualByteCount = false
        return formatter.string(fromByteCount: max(0, value))
    }

    /// "28.3 of 45.6 MB", or "900 KB of 45.6 MB" when the units differ.
    public static func bytes(_ done: Int64, of total: Int64) -> String {
        let doneText = bytes(done)
        let totalText = bytes(total)
        let doneParts = doneText.split(separator: " ")
        let totalParts = totalText.split(separator: " ")
        if doneParts.count == 2, totalParts.count == 2, doneParts[1] == totalParts[1] {
            return "\(doneParts[0]) of \(totalText)"
        }
        return "\(doneText) of \(totalText)"
    }

    public static func speed(_ bytesPerSecond: Double) -> String {
        bytes(Int64(bytesPerSecond.rounded())) + "/s"
    }

    /// "0:15 left", "12:04 left", "1:02:10 left".
    public static func timeLeft(_ seconds: Int) -> String {
        clock(seconds) + " left"
    }

    public static func clock(_ seconds: Int) -> String {
        let s = max(0, seconds)
        let hours = s / 3600
        let minutes = (s % 3600) / 60
        let secs = s % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, secs)
        }
        return String(format: "%d:%02d", minutes, secs)
    }

    public static func percent(_ fraction: Double) -> String {
        "\(Int((min(max(fraction, 0), 1) * 100).rounded(.down)))%"
    }

    /// Position in the queue: "Up next", "2nd in line", "3rd in line".
    public static func queuePosition(_ index: Int) -> String {
        index <= 1 ? "Up next" : "\(ordinal(index)) in line"
    }

    public static func ordinal(_ number: Int) -> String {
        let tens = number % 100
        let suffix: String
        if (11...13).contains(tens) {
            suffix = "th"
        } else {
            switch number % 10 {
            case 1: suffix = "st"
            case 2: suffix = "nd"
            case 3: suffix = "rd"
            default: suffix = "th"
            }
        }
        return "\(number)\(suffix)"
    }

    public static func ratio(_ value: Double) -> String {
        String(format: "%.2f", value)
    }

    public static func count(_ value: Int, _ singular: String, _ plural: String? = nil) -> String {
        "\(value) \(value == 1 ? singular : (plural ?? singular + "s"))"
    }
}
