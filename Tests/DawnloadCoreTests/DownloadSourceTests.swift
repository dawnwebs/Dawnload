import DawnloadCore
import XCTest

final class DownloadSourceTests: XCTestCase {
    func testWebLink() {
        let source = DownloadSource.parse("  https://www.youtube.com/watch?v=abc  ")
        XCTAssertEqual(source, .web(URL(string: "https://www.youtube.com/watch?v=abc")!))
        XCTAssertEqual(source?.shortName, "youtube.com")
        XCTAssertEqual(source?.provisionalTitle, "youtube.com/watch")
        XCTAssertEqual(source?.isTorrent, false)
    }

    func testRejectsTextThatIsNotALink() {
        XCTAssertNil(DownloadSource.parse("hello world"))
        XCTAssertNil(DownloadSource.parse("youtube.com/watch?v=abc"))
        XCTAssertNil(DownloadSource.parse("ftp://example.com/file"))
        XCTAssertNil(DownloadSource.parse(""))
    }

    func testMagnetLinkWithHexHash() throws {
        let text = "magnet:?xt=urn:btih:C9E15763F722F23E98A29DECDFAE341B98D53056&dn=Ubuntu+24.04+Desktop&tr=udp%3A%2F%2Ftracker.example%3A1337&xl=6114770944"
        let source = try XCTUnwrap(DownloadSource.parse(text))
        guard case .magnet(let magnet) = source else { return XCTFail("not a magnet") }
        XCTAssertEqual(magnet.infoHash, "c9e15763f722f23e98a29decdfae341b98d53056")
        XCTAssertEqual(magnet.displayName, "Ubuntu 24.04 Desktop")
        XCTAssertEqual(magnet.trackers, ["udp://tracker.example:1337"])
        XCTAssertEqual(magnet.exactLength, 6_114_770_944)
        XCTAssertEqual(source.shortName, "Ubuntu 24.04 Desktop")
        XCTAssertTrue(source.isTorrent)
    }

    func testMagnetLinkWithBase32HashAndNoName() throws {
        let text = "magnet:?xt=urn:btih:ZOCMZQIPFFW7OLLMIC5HUB6BPCSDEOQU"
        let magnet = try XCTUnwrap(MagnetLink(text))
        XCTAssertEqual(magnet.infoHash, "ZOCMZQIPFFW7OLLMIC5HUB6BPCSDEOQU")
        XCTAssertNil(magnet.displayName)
        XCTAssertEqual(DownloadSource.magnet(magnet).shortName, "Magnet link")
    }

    func testMagnetLinkWithoutValidHashIsRejected() {
        XCTAssertNil(DownloadSource.parse("magnet:?dn=nothing"))
        XCTAssertNil(DownloadSource.parse("magnet:?xt=urn:btih:1234"))
    }

    func testTorrentFilePaths() {
        XCTAssertEqual(
            DownloadSource.parse("/Users/me/Downloads/debian.torrent"),
            .torrentFile(URL(fileURLWithPath: "/Users/me/Downloads/debian.torrent"))
        )
        XCTAssertEqual(
            DownloadSource.parse("file:///tmp/a%20b.torrent"),
            .torrentFile(URL(fileURLWithPath: "/tmp/a b.torrent"))
        )
        XCTAssertNil(DownloadSource.parse("/Users/me/notes.txt"))
    }

    func testSourceRoundTripsThroughCodable() throws {
        let sources: [DownloadSource] = [
            .web(URL(string: "https://vimeo.com/1")!),
            .magnet(MagnetLink("magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=x")!),
            .torrentFile(URL(fileURLWithPath: "/tmp/x.torrent"))
        ]
        let data = try JSONEncoder().encode(sources)
        XCTAssertEqual(try JSONDecoder().decode([DownloadSource].self, from: data), sources)
    }

    func testAutoEngine() {
        func engine(_ text: String, _ preference: EnginePreference = .auto) -> EngineID? {
            DownloadSource.parse(text).map(preference.resolve)
        }
        XCTAssertEqual(engine("https://www.youtube.com/watch?v=abc"), .ytDlp)
        XCTAssertEqual(engine("https://www.instagram.com/p/xyz/"), .galleryDl)
        XCTAssertEqual(engine("https://www.instagram.com/reel/xyz/"), .ytDlp)
        XCTAssertEqual(engine("https://imgur.com/a/abc"), .galleryDl)
        XCTAssertEqual(engine("https://www.pixiv.net/en/artworks/1"), .galleryDl)
        XCTAssertEqual(engine("https://imgur.com/a/abc", .ytDlp), .ytDlp)
        XCTAssertEqual(engine("https://youtube.com/x", .galleryDl), .galleryDl)
        XCTAssertEqual(engine("magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056"), .torrent)
    }
}
