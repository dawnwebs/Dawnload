import DawnloadCore
import Foundation

/// A list in every state, for design review and screenshots. Launch with `-DawnloadDemo YES`
/// (add `-DawnloadTorrents YES` for the torrent states). Nothing runs and nothing is saved.
enum DemoData {
    @MainActor
    static func items(destination: URL, torrents: Bool) -> [DownloadItem] {
        var options = DownloadOptions(destination: destination)
        options.polish = true
        let folder = destination.path
        let now = Date()

        func web(_ link: String, _ title: String?, _ status: ItemStatus, minutesAgo: Double = 0) -> DownloadItem {
            let item = DownloadItem(source: DownloadSource.parse(link)!, options: options, status: status,
                                    createdAt: now.addingTimeInterval(-minutesAgo * 60))
            item.title = title
            return item
        }

        let pulling = web("https://vimeo.com/1084537", "Big Buck Bunny, 4K 60 fps", .pulling)
        pulling.transfer = TransferStats(fraction: 0.62, bytesDone: 28_300_000, bytesTotal: 45_600_000,
                                         bytesPerSecond: 2_340_000, secondsLeft: 15)
        pulling.command = "/opt/homebrew/bin/yt-dlp --newline --no-color --progress -P \(folder) -f 'bv*+ba/b' https://vimeo.com/1084537"
        pulling.log = [
            "Started 9 Nov 2026 at 00:12:04",
            "$ \(pulling.command!)",
            "[vimeo] Extracting URL: https://vimeo.com/1084537",
            "[info] 1084537: Downloading 1 format(s): 1080p+audio"
        ]

        let playlist = web("https://www.youtube.com/playlist?list=PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG", "Open Movie Shorts", .paused, minutesAgo: 30)
        playlist.options.playlist = true
        playlist.transfer = TransferStats(fraction: 0.35, bytesDone: 210_000_000, bytesTotal: 600_000_000, entryIndex: 4, entryCount: 12)

        let gallery = web("https://www.flickr.com/photos/nasahqphoto/albums/72177720313849849", "flickr / nasahqphoto", .polishing, minutesAgo: 2)
        gallery.phaseDetail = "faststart, file 3 of 9"
        gallery.transfer = TransferStats(fraction: 1)

        let queued1 = web("https://www.youtube.com/watch?v=aqz-KE-bpKQ", nil, .queued, minutesAgo: 1)
        let queued2 = web("https://soundcloud.com/dawnlabs/field-recording-03", "Field recording 03", .queued, minutesAgo: 1)
        queued2.options.quality = .audio
        let queued3 = web("https://www.instagram.com/p/C9xYz12AbcD/", nil, .queued, minutesAgo: 1)

        let kept1 = web("https://vimeo.com/253989945", "Sintel, the trailer", .kept, minutesAgo: 50)
        kept1.files = [ItemFile(path: "\(folder)/Sintel, the trailer.mp4", size: 62_400_000)]
        kept1.transfer = TransferStats(fraction: 1, bytesDone: 62_400_000, bytesTotal: 62_400_000)
        kept1.finishedAt = now.addingTimeInterval(-40 * 60)

        let kept2 = web("https://www.artstation.com/artwork/l4Ry8e", "artstation / dawn-studio", .kept, minutesAgo: 90)
        kept2.files = (1...9).map { ItemFile(path: "\(folder)/artstation/dawn-studio/plate-\($0).jpg", size: 1_800_000) }
        kept2.finishedAt = now.addingTimeInterval(-80 * 60)

        let failed = web("https://www.example-news.com/video/evening-report", "Evening report", .failed, minutesAgo: 20)
        failed.failure = DownloadFailure.classify(tool: "yt-dlp", exitCode: 1,
            output: "ERROR: [generic] Unable to download webpage: HTTP Error 403: Forbidden")
        failed.finishedAt = now.addingTimeInterval(-19 * 60)
        failed.log = [
            "Started 9 Nov 2026 at 23:52:10",
            "$ /opt/homebrew/bin/yt-dlp --newline https://www.example-news.com/video/evening-report",
            "[generic] Extracting URL: https://www.example-news.com/video/evening-report",
            "ERROR: [generic] Unable to download webpage: HTTP Error 403: Forbidden",
            "Failed: This site refused the request (HTTP 403). Turn on browser impersonation, or add your cookies.txt in Settings."
        ]

        var items = [pulling, gallery, queued1, queued2, queued3, playlist, kept1, kept2, failed]

        if torrents {
            let magnet = DownloadSource.parse(
                "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=Blender+Open+Movies+Collection")!
            let seeding = DownloadItem(source: magnet, options: options, status: .seeding, createdAt: now.addingTimeInterval(-3 * 3600))
            seeding.files = [
                ItemFile(path: "\(folder)/Blender Open Movies/Big Buck Bunny.mkv", size: 885_000_000),
                ItemFile(path: "\(folder)/Blender Open Movies/Sintel.mkv", size: 1_100_000_000),
                ItemFile(path: "\(folder)/Blender Open Movies/Tears of Steel.mkv", size: 1_400_000_000),
                ItemFile(path: "\(folder)/Blender Open Movies/Extras/Making of.mp4", size: 320_000_000, isSelected: false)
            ]
            seeding.transfer = TransferStats(fraction: 1, bytesDone: 3_385_000_000, bytesTotal: 3_385_000_000)
            seeding.swarm = SwarmStats(peers: 12, seeds: 48, downloadRate: 0, uploadRate: 1_200_000, uploaded: 2_843_000_000, ratio: 0.84)
            seeding.finishedAt = now.addingTimeInterval(-3600)

            let torrentFile = DownloadSource.torrentFile(URL(fileURLWithPath: "/Users/me/Downloads/debian-13.2.0-arm64-netinst.torrent"))
            let waitingTorrent = DownloadItem(source: torrentFile, options: options, status: .queued)
            waitingTorrent.files = [
                ItemFile(path: "debian-13.2.0-arm64-netinst.iso", size: 724_000_000),
                ItemFile(path: "SHA256SUMS", size: 2_048),
                ItemFile(path: "SHA256SUMS.sign", size: 833, isSelected: false)
            ]
            items.insert(waitingTorrent, at: 5)
            items.append(seeding)
        }

        return items
    }
}
