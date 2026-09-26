// PageLampSnapshots: renders every page of the snapshot catalogue to PNGs (light/dark ×
// en/zh-Hans) on mock data. Headless: no window, no app launch; ImageRenderer only.
//
//     swift run PageLampSnapshots [output-directory] [name-prefix …]
//
// Without a directory: $PAGELAMP_SNAPSHOT_DIR, else $TMPDIR/PageLampSnapshots. Name prefixes
// ("this-week", "course-DEMO101", "settings") render only the matching pages.

import Foundation
import PageLamp

let arguments = Array(CommandLine.arguments.dropFirst())
let directory: URL = if let path = arguments.first ?? ProcessInfo.processInfo.environment["PAGELAMP_SNAPSHOT_DIR"] {
    URL(filePath: path, directoryHint: .isDirectory)
} else {
    URL(filePath: NSTemporaryDirectory(), directoryHint: .isDirectory)
        .appending(path: "PageLampSnapshots", directoryHint: .isDirectory)
}

do {
    let rendered = try await SnapshotRenderer.renderAll(to: directory, prefixes: Array(arguments.dropFirst()))
    for image in rendered {
        print(image.file.path(percentEncoded: false))
    }
} catch {
    FileHandle.standardError.write(Data("PageLampSnapshots: \(error)\n".utf8))
    exit(1)
}
