import Foundation

enum ListFilter: String, CaseIterable, Identifiable {
    case all
    case active
    case waiting
    case kept
    case failed
    case seeding

    var id: String { rawValue }

    var title: String {
        switch self {
        case .all: "All"
        case .active: "Active"
        case .waiting: "Up next"
        case .kept: "Kept"
        case .failed: "Failed"
        case .seeding: "Seeding"
        }
    }

    func includes(_ status: ItemStatus) -> Bool {
        switch self {
        case .all: true
        case .active: status.isActive
        case .waiting: status == .queued || status == .paused
        case .kept: status == .kept
        case .failed: status == .failed
        case .seeding: status == .seeding
        }
    }

    var emptyLine: String {
        switch self {
        case .all: "Paste a link to start."
        case .active: "Nothing running."
        case .waiting: "Nothing waiting."
        case .kept: "Nothing kept yet."
        case .failed: "Nothing failed."
        case .seeding: "Nothing seeding."
        }
    }
}

/// Groups of the list, in display order.
enum ListSection: String, CaseIterable, Identifiable {
    case now
    case upNext
    case paused
    case seeding
    case done

    var id: String { rawValue }

    var title: String {
        switch self {
        case .now: "Now"
        case .upNext: "Up next"
        case .paused: "Paused"
        case .seeding: "Seeding"
        case .done: "Finished"
        }
    }

    static func of(_ status: ItemStatus) -> ListSection {
        switch status {
        case .pulling, .polishing: .now
        case .queued: .upNext
        case .paused: .paused
        case .seeding: .seeding
        case .kept, .failed: .done
        }
    }
}

enum AppearancePreference: String, CaseIterable, Identifiable {
    case system
    case dark
    case light

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: "System"
        case .dark: "Dark"
        case .light: "Light"
        }
    }
}
