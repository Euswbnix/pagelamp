// Where a line of meta text may wrap (spec §7.3: text wraps, never truncates): between the
// items of a "·" list, never inside a date, a time or a short label. A narrow column then shows
// "Thu, Oct 9 ·" / "6:00 PM · Assignment" instead of "Thu, Oct" / "9 · 6:00 PM · Assign…".

public enum TextWrap {
    /// No-break space (between words of one unit).
    public static let noBreakSpace: Character = "\u{00A0}"
    /// Word joiner: forbids a break between two characters without adding space (CJK text may
    /// otherwise break between any two characters).
    public static let wordJoiner: Character = "\u{2060}"

    /// `text` as one unbreakable unit: spaces become no-break spaces, and ideographs are joined
    /// so "10月9日 周四" stays on one line too.
    public static func keepTogether(_ text: String) -> String {
        var out = ""
        var previous: Character?
        for character in text {
            let current: Character = character == " " ? noBreakSpace : character
            if let previous, isIdeographic(previous) || isIdeographic(current) {
                out.append(wordJoiner)
            }
            out.append(current)
            previous = current
        }
        return out
    }

    /// Items joined by " · ", each kept together; a line may break only after a "·" (which stays
    /// with the item before it).
    public static func items(_ parts: [String]) -> String {
        parts.filter { !$0.isEmpty }.map(keepTogether).joined(separator: "\(noBreakSpace)· ")
    }

    /// Han, kana, Hangul and full-width forms: scripts that break between any two characters.
    private static func isIdeographic(_ character: Character) -> Bool {
        character.unicodeScalars.contains { scalar in
            switch scalar.value {
            case 0x2E80...0x9FFF, 0xAC00...0xD7AF, 0xF900...0xFAFF, 0xFF00...0xFFEF, 0x20000...0x3FFFF: true
            default: false
            }
        }
    }
}
