import XCTest
@testable import DawnloadCore

final class URLSanitizerTests: XCTestCase {
    func testPreservesSpankBangTitleSlug() {
        let input = "https://spankbang.com/77ii8/video/fc2+ppv+1106157+20+h+w"

        XCTAssertEqual(normalizedDownloadURL(from: input)?.absoluteString, input)
    }

    func testRemovesSpankBangTrackingData() {
        let input = "https://spankbang.com/77ii8/video/?utm_source=test#player"

        XCTAssertEqual(
            normalizedDownloadURL(from: input)?.absoluteString,
            "https://spankbang.com/77ii8/video/"
        )
    }

    func testPreservesOtherHTTPURLs() {
        let input = "https://example.com/profile/video/clip-name?quality=best"

        XCTAssertEqual(normalizedDownloadURL(from: input)?.absoluteString, input)
    }

    func testRejectsNonHTTPURLs() {
        XCTAssertNil(normalizedDownloadURL(from: "file:///tmp/video.mp4"))
        XCTAssertNil(normalizedDownloadURL(from: "not a URL"))
    }
}
