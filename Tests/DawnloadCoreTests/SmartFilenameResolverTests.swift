import XCTest
@testable import DawnloadCore

final class SmartFilenameResolverTests: XCTestCase {
    func testUsesRealExtractorTitle() {
        let metadata = SmartFilenameMetadata(
            id: "77ii8",
            title: "A Proper Page Title",
            webpageURL: "https://spankbang.com/77ii8/video/a+proper+page+title"
        )

        XCTAssertEqual(
            SmartFilenameResolver.bestTitle(metadata: metadata, sourceURL: ""),
            "A Proper Page Title"
        )
    }

    func testRejectsIDTitleAndUsesPreservedURLSlug() {
        let metadata = SmartFilenameMetadata(
            id: "77ii8",
            title: "77ii8",
            displayID: "77ii8",
            webpageURL: "https://spankbang.com/77ii8/video/fc2+ppv+1106157+20+h+w"
        )

        XCTAssertEqual(
            SmartFilenameResolver.bestTitle(metadata: metadata, sourceURL: ""),
            "FC2 PPV 1106157 20 H W"
        )
    }

    func testUsesSourceURLWhenMetadataURLHasNoTitleSlug() {
        let metadata = SmartFilenameMetadata(
            id: "77ii8",
            title: "77ii8",
            webpageURL: "https://spankbang.com/77ii8/video/"
        )

        XCTAssertEqual(
            SmartFilenameResolver.bestTitle(
                metadata: metadata,
                sourceURL: "https://spankbang.com/77ii8/video/better+fallback"
            ),
            "Better Fallback"
        )
    }

    func testFallsBackToReadableVideoIdentifier() {
        let metadata = SmartFilenameMetadata(id: "77ii8", title: "77ii8")

        XCTAssertEqual(
            SmartFilenameResolver.bestTitle(metadata: metadata, sourceURL: ""),
            "Video 77ii8"
        )
    }

    func testUsesUploaderAndFormattedDateBeforeIdentifier() {
        let metadata = SmartFilenameMetadata(
            id: "abc123",
            title: "abc123",
            uploader: "Road Studio",
            uploadDate: "20260724"
        )

        XCTAssertEqual(
            SmartFilenameResolver.bestTitle(metadata: metadata, sourceURL: ""),
            "Road Studio — 2026-07-24"
        )
    }
}
