// The small decisions behind the views: the materials list's keyboard and pointer handling
// (spec §3.2 "Mac list behaviour"), what opening a link does, what ⌘C copies, relative times in
// both languages (§2.3), where meta lines may wrap (§7.3), and the fix bubble's label colour
// (§6.2). Only the temporary folder is touched (for the launchable-file checks).

import Foundation
import PageLampKit
import PageLampModel
import Testing

private func material(_ id: String, url: String? = nil, title: String? = nil) -> MaterialView {
    MaterialView(
        id: id, courseId: "c", title: title ?? "Material \(id)", kind: .file, moduleId: nil, moduleName: nil,
        weekHint: 4, publishedAt: nil, url: url, textStatus: .ok, textError: nil,
        downloadBlocked: nil, chunkCount: 0
    )
}

@Suite("Materials list behaviour")
struct MaterialListTests {
    let ids = ["a", "b", "c"]

    @Test("Tab into the list selects the first row; ↑/↓ move and stop at the ends")
    func keyboardMovement() {
        var list = MaterialListSelection()
        #expect(list.handle(.focusEntered, in: ids) == nil)
        #expect(list.selected == "a")
        #expect(list.handle(.move(1), in: ids) == nil)
        #expect(list.selected == "b")
        _ = list.handle(.move(1), in: ids)
        _ = list.handle(.move(1), in: ids)
        #expect(list.selected == "c")
        _ = list.handle(.move(-1), in: ids)
        _ = list.handle(.move(-1), in: ids)
        _ = list.handle(.move(-1), in: ids)
        #expect(list.selected == "a")

        // Coming back to the list keeps the selection.
        _ = list.handle(.move(1), in: ids)
        _ = list.handle(.focusEntered, in: ids)
        #expect(list.selected == "b")

        // Without a selection, ↑ selects the last row.
        var fresh = MaterialListSelection()
        _ = fresh.handle(.move(-1), in: ids)
        #expect(fresh.selected == "c")

        // An empty list has nothing to select.
        var empty = MaterialListSelection()
        _ = empty.handle(.focusEntered, in: [])
        _ = empty.handle(.move(1), in: [])
        #expect(empty.selected == nil)
    }

    @Test("a click selects; a double-click or Return opens; Return with nothing selected passes on")
    func openingAndClicks() {
        var list = MaterialListSelection()
        // Return without a selection isn't handled: the key goes on to the window.
        #expect(list.handle(.returnKey, in: ids) == nil)

        #expect(list.handle(.click("b"), in: ids) == nil)
        #expect(list.selected == "b")
        #expect(list.handle(.returnKey, in: ids) == .open("b"))

        #expect(list.handle(.doubleClick("c"), in: ids) == .open("c"))
        #expect(list.selected == "c")

        // A row that is gone (another week loaded) opens nothing and selects nothing.
        #expect(list.handle(.doubleClick("zz"), in: ids) == nil)
        #expect(list.handle(.click("zz"), in: ids) == nil)
        #expect(list.selected == "c")
        #expect(list.handle(.returnKey, in: ["a", "b"]) == nil)
    }

    @Test("the selection goes when its material does (another week, a sync)")
    func keepsOnlyListed() {
        var list = MaterialListSelection(selected: "b")
        list.keep(in: ids)
        #expect(list.selected == "b")
        list.keep(in: ["a", "c"])
        #expect(list.selected == nil)
        // Tab back in: the first row again.
        _ = list.handle(.focusEntered, in: ["a", "c"])
        #expect(list.selected == "a")
    }

    @Test("⌘C copies the selected material's path, web address or title")
    func copy() {
        let materials = [
            material("file", url: "file:///Users/demo/Documents/Courses/DEMO101/Week%204/slides%20v2.pdf"),
            material("web", url: "https://canvas.demo.test/courses/101/files/7"),
            material("none", title: "Scanned handout"),
            material("script", url: "javascript:alert(1)", title: "Odd link"),
        ]
        var list = MaterialListSelection()
        #expect(list.copyItems(materials).isEmpty)
        list.select("file")
        #expect(list.copyItems(materials) == ["/Users/demo/Documents/Courses/DEMO101/Week 4/slides v2.pdf"])
        list.select("web")
        #expect(list.copyItems(materials) == ["https://canvas.demo.test/courses/101/files/7"])
        list.select("none")
        #expect(list.copyItems(materials) == ["Scanned handout"])
        // A link PageLamp never opens is not copied as one either.
        list.select("script")
        #expect(list.copyItems(materials) == ["Odd link"])
    }
}

@Suite("Opening a material or deadline link")
struct LinkActionTests {
    @Test("web addresses go to the browser; documents open; programs are shown in Finder")
    func byName() throws {
        let web = try #require(CourseLink("https://canvas.demo.test/courses/101"))
        #expect(LinkAction.of(web) == .openWeb(web.url))

        let never: (URL) -> Bool = { _ in false }
        let always: (URL) -> Bool = { _ in true }
        let pdf = try #require(CourseLink("file:///Users/demo/Courses/notes.pdf"))
        #expect(LinkAction.of(pdf, isLaunchable: never) == .open(pdf.url))
        #expect(LinkAction.of(pdf, isLaunchable: always) == .reveal(pdf.url))

        // By extension, even for a file that isn't there.
        for name in ["Tool.app", "setup.command", "install.sh", "grader.py", "Lab.jar", "Driver.pkg", "Run.workflow"] {
            let url = URL(filePath: "/nonexistent/PageLampTests/\(name)")
            #expect(LocalFile.isLaunchable(url), "\(name)")
            #expect(LinkAction.of(.file(url)) == .reveal(url))
        }
        for name in ["notes.pdf", "slides.key", "reading.docx", "data.csv", "index.html"] {
            #expect(!LocalFile.isLaunchable(URL(filePath: "/nonexistent/PageLampTests/\(name)")), "\(name)")
        }
    }

    @Test("an executable without an extension is shown in Finder; a document with the executable bit opens")
    func byType() throws {
        let folder = URL(filePath: NSTemporaryDirectory(), directoryHint: .isDirectory)
            .appending(path: "PageLampLinkTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }

        let tool = folder.appending(path: "grade-me")
        try Data("#!/bin/sh\necho hi\n".utf8).write(to: tool)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: tool.path(percentEncoded: false))
        #expect(LocalFile.isLaunchable(tool))
        #expect(LinkAction.of(.file(tool)) == .reveal(tool))

        // A PDF copied from an exFAT drive keeps an executable bit: still a document.
        let pdf = folder.appending(path: "week4.pdf")
        try Data("%PDF-1.4\n".utf8).write(to: pdf)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: pdf.path(percentEncoded: false))
        #expect(!LocalFile.isLaunchable(pdf))

        // A link to a program counts as the program, whatever the link is called.
        let link = folder.appending(path: "notes-link.pdf")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: tool)
        #expect(LocalFile.isLaunchable(link))
        #expect(LocalFile.isLaunchable(URL(filePath: "/nonexistent/PageLampTests/shortcut.webloc")))
        #expect(LinkAction.of(.file(pdf)) == .open(pdf))

        // A plain text file without an extension or executable bit opens.
        let readme = folder.appending(path: "README")
        try Data("read me\n".utf8).write(to: readme)
        #expect(!LocalFile.isLaunchable(readme))
    }
}

@Suite("Relative times (spec §2.3)")
struct RelativeTimeTests {
    static let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "America/Toronto") ?? .gmt
        return calendar
    }()
    /// Friday 2026-09-25 10:00.
    static let now = calendar.date(from: DateComponents(year: 2026, month: 9, day: 25, hour: 10)) ?? Date()

    let en = L10n(locale: Locale(identifier: "en_US"), table: .app)
    let zh = L10n(locale: Locale(identifier: "zh-Hans_CN"), table: .app)

    private func ago(_ l10n: L10n, _ seconds: TimeInterval) -> String {
        l10n.relative(Self.now.addingTimeInterval(-seconds), to: Self.now, calendar: Self.calendar)
    }

    @Test("numbers and short units: min, h, days / 分钟, 小时, 天")
    func words() {
        let cases: [(TimeInterval, String, String)] = [
            (20, "1 min ago", "1 分钟前"),             // never "now": under a minute is a minute
            (5 * 60, "5 min ago", "5 分钟前"),
            (59 * 60, "59 min ago", "59 分钟前"),
            (2 * 3_600, "2 h ago", "2 小时前"),
            (2 * 3_600 + 20 * 60, "2 h ago", "2 小时前"), // rounded
            (23 * 3_600, "23 h ago", "23 小时前"),
            (3 * 86_400, "3 days ago", "3 天前"),
        ]
        for (seconds, english, chinese) in cases {
            #expect(ago(en, seconds) == english)
            #expect(ago(zh, seconds) == chinese)
        }
    }

    @Test("a day or more counts calendar days; one day is singular in English")
    func days() {
        // Wednesday 16:00 → Friday 10:00 is 42 hours but two calendar days.
        #expect(ago(en, 42 * 3_600) == "2 days ago")
        #expect(ago(zh, 42 * 3_600) == "2 天前")
        // Thursday 09:00 → Friday 10:00: one day.
        #expect(ago(en, 25 * 3_600) == "1 day ago")
        #expect(ago(zh, 25 * 3_600) == "1 天前")
    }

    @Test("a date after now (clock skew) reads as a minute ago")
    func future() {
        #expect(ago(en, -300) == "1 min ago")
        #expect(ago(zh, -300) == "1 分钟前")
    }
}

@Suite("Where meta lines may wrap (spec §7.3)")
struct TextWrapTests {
    @Test("items break only after a ·, never inside a date or a time")
    func items() {
        let line = TextWrap.items(["Thu, Oct 9", "6:00 PM", "", "Assignment"])
        #expect(line == "Thu,\u{00A0}Oct\u{00A0}9\u{00A0}· 6:00\u{00A0}PM\u{00A0}· Assignment")
        // The only breakable spaces are the ones after a "·".
        let breakable = line.enumerated().filter { $0.element == " " }.map(\.offset)
        let chars = Array(line)
        #expect(breakable.allSatisfy { chars[$0 - 1] == "·" })
    }

    @Test("Chinese dates stay on one line too")
    func chinese() {
        let kept = TextWrap.keepTogether("10月9日 周四")
        #expect(!kept.contains(" "))
        #expect(kept.replacingOccurrences(of: "\u{2060}", with: "").replacingOccurrences(of: "\u{00A0}", with: " ") == "10月9日 周四")
        // A joiner between every two characters where one is an ideograph.
        #expect(kept.contains("月\u{2060}9"))
        #expect(kept.contains("周\u{2060}四"))
        #expect(TextWrap.keepTogether("DEMO101") == "DEMO101")
    }
}

@Suite("The fix bubble's label on the accent colour (spec §6.2)")
struct ProminentLabelTests {
    /// The macOS accent colours (light, then dark appearance), sRGB 0…255.
    static let whiteLabel: [(String, Double, Double, Double)] = [
        ("blue", 0, 122, 255), ("blue dark", 10, 132, 255),
        ("purple", 175, 82, 222), ("purple dark", 191, 90, 242),
        ("pink", 255, 45, 85), ("pink dark", 255, 55, 95),
        ("red", 255, 59, 48), ("red dark", 255, 69, 58),
        ("graphite", 140, 140, 140),
    ]
    static let blackLabel: [(String, Double, Double, Double)] = [
        ("orange", 255, 149, 0), ("orange dark", 255, 159, 10),
        ("yellow", 255, 204, 0), ("yellow dark", 255, 214, 10),
        ("green", 40, 205, 65), ("green dark", 50, 215, 75),
    ]

    @Test("white on blue, purple, pink, red and graphite; black on yellow, orange and green")
    func accents() {
        for (name, r, g, b) in Self.whiteLabel {
            #expect(!ProminentLabel.prefersDarkText(red: r / 255, green: g / 255, blue: b / 255), "\(name)")
            #expect(ProminentLabel.contrastOfWhite(red: r / 255, green: g / 255, blue: b / 255) >= 3, "\(name)")
        }
        for (name, r, g, b) in Self.blackLabel {
            #expect(ProminentLabel.prefersDarkText(red: r / 255, green: g / 255, blue: b / 255), "\(name)")
        }
    }

    @Test("luminance follows WCAG 2")
    func luminance() {
        #expect(ProminentLabel.relativeLuminance(red: 0, green: 0, blue: 0) == 0)
        #expect(abs(ProminentLabel.relativeLuminance(red: 1, green: 1, blue: 1) - 1) < 1e-9)
        #expect(abs(ProminentLabel.contrastOfWhite(red: 0, green: 0, blue: 0) - 21) < 1e-9)
        #expect(ProminentLabel.prefersDarkText(red: 1, green: 1, blue: 1))
        #expect(!ProminentLabel.prefersDarkText(red: 0, green: 0, blue: 0))
    }
}
