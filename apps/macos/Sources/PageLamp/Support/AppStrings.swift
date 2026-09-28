// The app's string table (the generated .lproj in this target's resource bundle) and small
// helpers for text and glyphs every screen needs. The code → words helpers (`l10n.policy(_:)`,
// `l10n.sourceError(_:)`, …) live in PageLampModel (L10n+Formatting.swift).

import SwiftUI
import PageLampKit
import PageLampModel

extension StringTable {
    /// PageLamp's generated strings: `Resources/{en,zh-Hans}.lproj` + `Generated/L10nKeys.swift`.
    public nonisolated static let app = StringTable(
        bundleURL: Bundle.module.bundleURL,
        keys: L10nKeys.all,
        plural: L10nKeys.plural,
        arguments: L10nKeys.arguments
    )
}

extension Text {
    /// The core's English (progress, errors, warnings, evidence, notes), shown verbatim where
    /// Tauri shows it and tagged English, so VoiceOver can read it with an English voice in any
    /// UI language (spec §7.1 [verify]).
    static func english(_ string: String) -> Text {
        var attributed = AttributedString(string)
        attributed.languageIdentifier = "en"
        return Text(attributed)
    }
}

/// Sentences built from words and inline glyphs: one Text that wraps like a sentence (and
/// measures the same in every layout pass, unlike a row that switches layouts).
enum InlineText {
    static func joined(_ parts: [Text]) -> Text {
        parts.reduce(Text(verbatim: "")) { Text("\($0)\($1)") }
    }

    /// An inline SF Symbol in a sentence, in a neutral (secondary) colour.
    static func glyph(_ symbol: String) -> Text {
        Text(Image(systemName: symbol)).foregroundStyle(.secondary)
    }
}

extension AiPolicy {
    /// The policy's neutral SF Symbol (spec §4.5).
    var symbol: String {
        switch self {
        case .unknown: "questionmark.circle"
        case .prohibited: "hand.raised"
        case .learningAid: "lightbulb"
        case .allowedWithCitation: "quote.opening"
        case .unrestricted: "checkmark.circle"
        }
    }
}

extension SourceKind {
    /// The source kind's SF Symbol (spec §4.5).
    var symbol: String {
        switch self {
        case .canvas: "graduationcap"
        case .folder: "folder"
        case .ical: "calendar"
        }
    }
}

extension MaterialKind {
    /// The material kind's SF Symbol (spec §3.2 MaterialRow).
    var symbol: String {
        switch self {
        case .file: "doc.text"
        case .page: "doc.richtext"
        case .syllabus: "text.book.closed"
        case .announcement: "megaphone"
        case .externalLink: "link"
        }
    }
}

/// External links of the default brand (apps/desktop/src/brand/brands/default.ts).
enum BrandLinks {
    static let homepage = URL(string: "https://github.com/Euswbnix/pagelamp")
    static let issues = URL(string: "https://github.com/Euswbnix/pagelamp/issues/new/choose")
}
