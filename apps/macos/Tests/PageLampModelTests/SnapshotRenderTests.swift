// Renders the snapshot catalogue (every page of every screen, light/dark × en/zh-Hans, headless
// with ImageRenderer) when asked:
//
//     PAGELAMP_SNAPSHOT_DIR=/path/to/dir swift test --filter SnapshotRenderTests
//     PAGELAMP_SNAPSHOT_FILTER=this-week,course-DEMO101 …   (name prefixes, optional)
//
// Skipped otherwise: rendering holds the main actor for seconds, which starves the timing-based
// tests running in parallel. In debug builds an unknown string key or a missing argument trips an
// assertion, so a render also proves every page's strings exist in both languages.

import Foundation
import PageLamp
import Testing

@Suite("Snapshot renders", .serialized)
@MainActor
struct SnapshotRenderTests {
    nonisolated static let directory = ProcessInfo.processInfo.environment["PAGELAMP_SNAPSHOT_DIR"]
    nonisolated static let prefixes = ProcessInfo.processInfo.environment["PAGELAMP_SNAPSHOT_FILTER"]?
        .split(separator: ",").map(String.init) ?? []

    @Test("every page renders in both languages and appearances", .enabled(if: directory != nil))
    func renderAll() async throws {
        let directory = URL(filePath: try #require(Self.directory), directoryHint: .isDirectory)
        let rendered = try await SnapshotRenderer.renderAll(to: directory, prefixes: Self.prefixes)
        let pages = SnapshotCatalog.pages.filter { page in
            Self.prefixes.isEmpty || Self.prefixes.contains { page.name.hasPrefix($0) }
        }
        #expect(rendered.count == pages.count * SnapshotRenderer.variants.count)
        for image in rendered {
            let size = try FileManager.default.attributesOfItem(atPath: image.file.path(percentEncoded: false))[.size] as? Int
            #expect((size ?? 0) > 10_000, "\(image.name) looks empty")
            #expect(image.width > 0 && image.height > 0)
            print(image.file.path(percentEncoded: false))
        }
    }
}
