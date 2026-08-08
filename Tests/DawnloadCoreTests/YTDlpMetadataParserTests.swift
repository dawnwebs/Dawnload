import XCTest
@testable import DawnloadCore

final class YTDlpMetadataParserTests: XCTestCase {
    func testExtractsMetadataMarkerFromMixedProcessOutput() {
        let output = """
        [download] 100% of 1.00MiB
        __DAWNLOAD_METADATA__{"id":"77ii8","title":"77ii8","webpage_url":"https://spankbang.com/77ii8/video/fc2+ppv","filepath":"/tmp/77ii8.mp4"}
        [Merger] Finished
        """

        XCTAssertEqual(
            YTDlpMetadataParser.records(from: output),
            [
                SmartFilenameMetadata(
                    id: "77ii8",
                    title: "77ii8",
                    webpageURL: "https://spankbang.com/77ii8/video/fc2+ppv",
                    filepath: "/tmp/77ii8.mp4"
                )
            ]
        )
    }

    func testIgnoresMalformedMetadataLines() {
        let output = """
        __DAWNLOAD_METADATA__not-json
        ordinary output
        """

        XCTAssertTrue(YTDlpMetadataParser.records(from: output).isEmpty)
    }
}
