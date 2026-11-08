import DawnloadCore
import XCTest

final class YTDlpProgressTests: XCTestCase {
    // Lines captured from yt-dlp 2026.07.04 with YTDlpMarkers.arguments.
    private let mergedRun = [
        #"__DAWNLOAD_ITEM__{"id": "jNQXAC9IVRw", "title": "Me at the zoo", "format_id": "395+251", "filesize_approx": 475961}"#,
        #"__DAWNLOAD_PARTS__[{"format_id": "395", "filesize": 223779, "filesize_approx": 223778}, {"format_id": "251", "filesize": 252182, "filesize_approx": 252180}]"#,
        "__DAWNLOAD_PROGRESS__1024 223779 NA 616119.25 0 395 downloading",
        "__DAWNLOAD_PROGRESS__223779 223779 NA 1742554.12 NA 395 finished",
        "__DAWNLOAD_PROGRESS__126091 252182 NA 1816425.13 0 251 downloading",
        "__DAWNLOAD_PROGRESS__252182 252182 NA 1816425.13 NA 251 finished",
        "__DAWNLOAD_POST__started Merger"
    ]

    func testParsesEachMarker() {
        XCTAssertEqual(YTDlpLine.parse(mergedRun[0]), .item(YTDlpItemInfo(
            id: "jNQXAC9IVRw", title: "Me at the zoo", formatID: "395+251", filesizeApprox: 475_961)))
        XCTAssertEqual(YTDlpLine.parse(mergedRun[1]), .parts([
            YTDlpPart(formatID: "395", filesize: 223_779, filesizeApprox: 223_778),
            YTDlpPart(formatID: "251", filesize: 252_182, filesizeApprox: 252_180)
        ]))
        XCTAssertEqual(YTDlpLine.parse(mergedRun[2]), .progress(YTDlpProgressLine(
            downloaded: 1024, total: 223_779, totalEstimate: nil, speed: 616_119.25, eta: 0, formatID: "395", finished: false)))
        XCTAssertEqual(YTDlpLine.parse(mergedRun[6]), .postprocess(started: true, name: "Merger", isPolish: true))
        XCTAssertEqual(YTDlpLine.parse("__DAWNLOAD_POST__started MoveFiles"), .postprocess(started: true, name: "MoveFiles", isPolish: false))
        XCTAssertEqual(YTDlpLine.parse("__DAWNLOAD_PARTS__NA"), .parts([]))
        XCTAssertEqual(YTDlpLine.parse("[youtube] Extracting URL"), .text("[youtube] Extracting URL"))
    }

    func testProgressAddsUpAcrossVideoAndAudioParts() throws {
        var tracker = YTDlpProgressTracker()
        let samples = mergedRun.compactMap { tracker.consume(YTDlpLine.parse($0)) }
        XCTAssertEqual(tracker.title, "Me at the zoo")
        XCTAssertEqual(samples.count, 4)

        let total: Int64 = 223_779 + 252_182
        XCTAssertEqual(samples[0].bytesTotal, total)
        XCTAssertEqual(samples[0].bytesDone, 1024)
        XCTAssertEqual(try XCTUnwrap(samples[1].fraction), 223_779.0 / Double(total), accuracy: 0.0001)
        // The audio part continues from where the video left off; the fill never goes backwards.
        XCTAssertEqual(samples[2].bytesDone, 223_779 + 126_091)
        XCTAssertGreaterThan(try XCTUnwrap(samples[2].fraction), try XCTUnwrap(samples[1].fraction))
        XCTAssertEqual(samples[3].fraction, 1)
        XCTAssertNil(samples[3].secondsLeft)
    }

    func testSingleFormatWithEstimateOnly() throws {
        var tracker = YTDlpProgressTracker()
        _ = tracker.consume(.parse(#"__DAWNLOAD_ITEM__{"title": "Clip", "format_id": "18"}"#))
        _ = tracker.consume(.parse("__DAWNLOAD_PARTS__NA"))
        let sample = try XCTUnwrap(tracker.consume(.parse("__DAWNLOAD_PROGRESS__500 NA 1000.0 100.0 NA 18 downloading")))
        XCTAssertEqual(sample.fraction, 0.5)
        XCTAssertEqual(sample.bytesTotal, 1000)
        XCTAssertEqual(sample.secondsLeft, 5)
    }

    func testUnknownSizeHasNoFraction() throws {
        var tracker = YTDlpProgressTracker()
        let sample = try XCTUnwrap(tracker.consume(.parse("__DAWNLOAD_PROGRESS__500 NA NA NA NA 18 downloading")))
        XCTAssertNil(sample.fraction)
        XCTAssertEqual(sample.bytesDone, 500)
        XCTAssertNil(sample.bytesPerSecond)
    }

    func testPlaylistProgressCountsEntries() throws {
        var tracker = YTDlpProgressTracker()
        _ = tracker.consume(.parse(#"__DAWNLOAD_ITEM__{"title": "Part 3", "playlist_index": 3, "n_entries": 4, "format_id": "18", "filesize": 100}"#))
        let sample = try XCTUnwrap(tracker.consume(.parse("__DAWNLOAD_PROGRESS__50 100 NA 10 NA 18 downloading")))
        XCTAssertEqual(try XCTUnwrap(sample.fraction), (2 + 0.5) / 4, accuracy: 0.0001)
        XCTAssertEqual(sample.entryIndex, 3)
        XCTAssertEqual(sample.entryCount, 4)
    }

    func testFailureClassification() {
        XCTAssertEqual(DownloadFailure.classify(tool: "yt-dlp", exitCode: 1,
            output: "ERROR: [generic] Unable to download webpage: HTTP Error 403: Forbidden").kind, .forbidden)
        XCTAssertEqual(DownloadFailure.classify(tool: "yt-dlp", exitCode: 1,
            output: "ERROR: [youtube] x: Sign in to confirm you’re not a bot. Use --cookies-from-browser").kind, .signInNeeded)
        XCTAssertEqual(DownloadFailure.classify(tool: "yt-dlp", exitCode: 1,
            output: "ERROR: Unsupported URL: https://example.com").kind, .unsupportedSite)
        XCTAssertEqual(DownloadFailure.classify(tool: "gallery-dl", exitCode: 64,
            output: "[gallery-dl][error] No suitable extractor found for 'https://x'").hint, "Try yt-dlp for video and audio.")

        let other = DownloadFailure.classify(tool: "yt-dlp", exitCode: 2, output: "noise\nERROR: something odd\n")
        XCTAssertEqual(other.kind, .other)
        XCTAssertEqual(other.message, "yt-dlp stopped with an error (exit 2).")
        XCTAssertEqual(other.detail, "ERROR: something odd")

        let forbidden = DownloadFailure.classify(tool: "yt-dlp", exitCode: 1, output: "HTTP Error 403")
        XCTAssertEqual(forbidden.sentence,
            "This site refused the request (HTTP 403). Turn on browser impersonation, or add your cookies.txt in Settings.")
    }

    func testFormatting() {
        XCTAssertEqual(DLFormat.timeLeft(15), "0:15 left")
        XCTAssertEqual(DLFormat.clock(3730), "1:02:10")
        XCTAssertEqual(DLFormat.percent(0.629), "62%")
        XCTAssertEqual(DLFormat.queuePosition(1), "Up next")
        XCTAssertEqual(DLFormat.queuePosition(2), "2nd in line")
        XCTAssertEqual(DLFormat.queuePosition(3), "3rd in line")
        XCTAssertEqual(DLFormat.queuePosition(11), "11th in line")
        XCTAssertEqual(DLFormat.queuePosition(22), "22nd in line")
        XCTAssertEqual(DLFormat.count(1, "file"), "1 file")
        XCTAssertEqual(DLFormat.count(3, "file"), "3 files")
        // Locale decides the decimal mark; the unit appears once when both share it.
        let pair = DLFormat.bytes(28_300_000, of: 45_600_000)
        XCTAssertTrue(pair.hasPrefix("28"), pair)
        XCTAssertEqual(pair.components(separatedBy: "MB").count, 2, pair)
        XCTAssertEqual(DLFormat.bytes(900_000, of: 45_600_000).components(separatedBy: "B").count, 3)
    }
}
