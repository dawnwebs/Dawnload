import Foundation

public enum YTDlpMetadataParser {
    public static let marker = "__DAWNLOAD_METADATA__"

    public static func records(from output: String) -> [SmartFilenameMetadata] {
        let decoder = JSONDecoder()

        return output.components(separatedBy: .newlines).compactMap { line in
            guard let markerRange = line.range(of: marker) else {
                return nil
            }

            let json = String(line[markerRange.upperBound...])
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard let data = json.data(using: .utf8) else {
                return nil
            }

            return try? decoder.decode(SmartFilenameMetadata.self, from: data)
        }
    }
}
