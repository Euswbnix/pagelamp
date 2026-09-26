// A small JSON reader and writer that keeps object keys in their written order. Connect uses it
// to cut the "mcpServers" entry out of a config snippet and print it the way the Tauri app does
// (`JSON.stringify(value, null, 2)`), which Foundation's JSONSerialization can't: it forgets
// the key order and prints `"key" : value`. Only for small, trusted snippets from the core.

import Foundation

/// A JSON value whose objects keep their key order. Numbers keep their source text.
public enum OrderedJSON: Equatable, Sendable {
    case object([(key: String, value: OrderedJSON)])
    case array([OrderedJSON])
    case string(String)
    case number(String)
    case bool(Bool)
    case null

    public static func == (lhs: OrderedJSON, rhs: OrderedJSON) -> Bool {
        switch (lhs, rhs) {
        case (.object(let a), .object(let b)):
            a.count == b.count && zip(a, b).allSatisfy { $0.key == $1.key && $0.value == $1.value }
        case (.array(let a), .array(let b)): a == b
        case (.string(let a), .string(let b)): a == b
        case (.number(let a), .number(let b)): a == b
        case (.bool(let a), .bool(let b)): a == b
        case (.null, .null): true
        default: false
        }
    }

    /// Parses a complete JSON text; nil if it isn't valid JSON.
    public init?(parsing text: String) {
        var parser = Parser(scalars: Array(text.unicodeScalars))
        guard let value = parser.parseDocument() else { return nil }
        self = value
    }

    /// The members of an object, in order (nil for anything else).
    public var members: [(key: String, value: OrderedJSON)]? {
        if case .object(let members) = self { return members }
        return nil
    }

    /// The value of `key` in an object.
    public subscript(key: String) -> OrderedJSON? {
        members?.first { $0.key == key }?.value
    }

    /// Printed like `JSON.stringify(value, null, 2)`: two-space indent, `"key": value`, empty
    /// containers as `{}` / `[]`. `level` indents nested lines (the first line is not indented).
    public func pretty(level: Int = 0) -> String {
        let indent = String(repeating: "  ", count: level + 1)
        let closing = String(repeating: "  ", count: level)
        switch self {
        case .object(let members):
            guard !members.isEmpty else { return "{}" }
            let lines = members.map { "\(indent)\(Self.quote($0.key)): \($0.value.pretty(level: level + 1))" }
            return "{\n" + lines.joined(separator: ",\n") + "\n\(closing)}"
        case .array(let values):
            guard !values.isEmpty else { return "[]" }
            let lines = values.map { "\(indent)\($0.pretty(level: level + 1))" }
            return "[\n" + lines.joined(separator: ",\n") + "\n\(closing)]"
        case .string(let text):
            return Self.quote(text)
        case .number(let text):
            return text
        case .bool(let value):
            return value ? "true" : "false"
        case .null:
            return "null"
        }
    }

    /// A JSON string literal, escaped like `JSON.stringify` (no escaping of "/" or non-ASCII).
    public static func quote(_ text: String) -> String {
        var out = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\u{08}": out += "\\b"
            case "\u{0C}": out += "\\f"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case _ where scalar.value < 0x20:
                out += String(format: "\\u%04x", scalar.value)
            default:
                out.unicodeScalars.append(scalar)
            }
        }
        return out + "\""
    }
}

/// Recursive-descent parser over Unicode scalars (RFC 8259, no extensions).
private struct Parser {
    let scalars: [Unicode.Scalar]
    var index = 0
    /// Deep nesting is never legitimate in a config snippet.
    private let maxDepth = 64

    init(scalars: [Unicode.Scalar]) {
        self.scalars = scalars
    }

    mutating func parseDocument() -> OrderedJSON? {
        skipWhitespace()
        guard let value = parseValue(depth: 0) else { return nil }
        skipWhitespace()
        return index == scalars.count ? value : nil
    }

    private var current: Unicode.Scalar? {
        index < scalars.count ? scalars[index] : nil
    }

    private mutating func skipWhitespace() {
        while let c = current, c == " " || c == "\n" || c == "\r" || c == "\t" {
            index += 1
        }
    }

    private mutating func consume(_ literal: String) -> Bool {
        let expected = Array(literal.unicodeScalars)
        guard index + expected.count <= scalars.count,
              Array(scalars[index ..< index + expected.count]) == expected
        else { return false }
        index += expected.count
        return true
    }

    private mutating func parseValue(depth: Int) -> OrderedJSON? {
        guard depth < maxDepth, let c = current else { return nil }
        switch c {
        case "{": return parseObject(depth: depth)
        case "[": return parseArray(depth: depth)
        case "\"": return parseString().map(OrderedJSON.string)
        case "t": return consume("true") ? .bool(true) : nil
        case "f": return consume("false") ? .bool(false) : nil
        case "n": return consume("null") ? .null : nil
        default: return parseNumber()
        }
    }

    private mutating func parseObject(depth: Int) -> OrderedJSON? {
        index += 1 // {
        var members: [(key: String, value: OrderedJSON)] = []
        skipWhitespace()
        if current == "}" {
            index += 1
            return .object(members)
        }
        while true {
            skipWhitespace()
            guard current == "\"", let key = parseString() else { return nil }
            skipWhitespace()
            guard current == ":" else { return nil }
            index += 1
            skipWhitespace()
            guard let value = parseValue(depth: depth + 1) else { return nil }
            members.append((key, value))
            skipWhitespace()
            if current == "," {
                index += 1
            } else if current == "}" {
                index += 1
                return .object(members)
            } else {
                return nil
            }
        }
    }

    private mutating func parseArray(depth: Int) -> OrderedJSON? {
        index += 1 // [
        var values: [OrderedJSON] = []
        skipWhitespace()
        if current == "]" {
            index += 1
            return .array(values)
        }
        while true {
            skipWhitespace()
            guard let value = parseValue(depth: depth + 1) else { return nil }
            values.append(value)
            skipWhitespace()
            if current == "," {
                index += 1
            } else if current == "]" {
                index += 1
                return .array(values)
            } else {
                return nil
            }
        }
    }

    private mutating func parseString() -> String? {
        index += 1 // opening quote
        var out = String.UnicodeScalarView()
        while let c = current {
            index += 1
            switch c {
            case "\"":
                return String(out)
            case "\\":
                guard let escaped = current else { return nil }
                index += 1
                switch escaped {
                case "\"": out.append("\"")
                case "\\": out.append("\\")
                case "/": out.append("/")
                case "b": out.append("\u{08}")
                case "f": out.append("\u{0C}")
                case "n": out.append("\n")
                case "r": out.append("\r")
                case "t": out.append("\t")
                case "u":
                    guard let scalar = parseUnicodeEscape() else { return nil }
                    out.append(scalar)
                default:
                    return nil
                }
            default:
                guard c.value >= 0x20 else { return nil }
                out.append(c)
            }
        }
        return nil
    }

    /// After `\u`: four hex digits, or a surrogate pair written as two escapes.
    private mutating func parseUnicodeEscape() -> Unicode.Scalar? {
        guard let high = parseHex4() else { return nil }
        if (0xD800 ... 0xDBFF).contains(high) {
            guard consume("\\u"), let low = parseHex4(), (0xDC00 ... 0xDFFF).contains(low) else { return nil }
            return Unicode.Scalar(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))
        }
        return Unicode.Scalar(high)
    }

    private mutating func parseHex4() -> UInt32? {
        guard index + 4 <= scalars.count else { return nil }
        var value: UInt32 = 0
        for _ in 0 ..< 4 {
            let scalar = scalars[index]
            guard scalar.isASCIIDigit || scalar.isASCIIHexLetter,
                  let digit = UInt32(String(scalar), radix: 16)
            else { return nil }
            value = value * 16 + digit
            index += 1
        }
        return value
    }

    private mutating func parseNumber() -> OrderedJSON? {
        let start = index
        if current == "-" { index += 1 }
        guard let first = current, first.isASCIIDigit else { return nil }
        if first == "0" {
            index += 1
        } else {
            while let c = current, c.isASCIIDigit { index += 1 }
        }
        if current == "." {
            index += 1
            guard let c = current, c.isASCIIDigit else { return nil }
            while let c = current, c.isASCIIDigit { index += 1 }
        }
        if current == "e" || current == "E" {
            index += 1
            if current == "+" || current == "-" { index += 1 }
            guard let c = current, c.isASCIIDigit else { return nil }
            while let c = current, c.isASCIIDigit { index += 1 }
        }
        var text = String.UnicodeScalarView()
        text.append(contentsOf: scalars[start ..< index])
        return .number(String(text))
    }
}

private extension Unicode.Scalar {
    var isASCIIDigit: Bool { ("0" ... "9").contains(self) }
    var isASCIIHexLetter: Bool { ("a" ... "f").contains(self) || ("A" ... "F").contains(self) }
}
