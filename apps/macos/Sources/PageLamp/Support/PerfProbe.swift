// Performance probe (a debug tool; does nothing unless PAGELAMP_PERF_PROBE is set).
//
// PAGELAMP_PERF_PROBE=inspector | sidebar | navigate | courses | enter | all opens a course and
// repeats the interaction exactly as the UI does it (the inspector button, the sidebar toggle,
// page switches), records every display frame with the window's display link, prints one JSON
// line (median first-frame delay, longest frame, hitch ms per second, main-thread CPU time, where
// the long frames were) and quits. Panel-animation jank has no unit-test seam (it lives in
// AppKit's window layout), so run scripts/perf-probe.sh on a release build before and after
// layout changes.
//
// PAGELAMP_PERF_PROBE=capsule drives the sidebar with real input events (clicks with a 0 and a
// 50 ms press, arrows, a held arrow, ⌘1/2/3, a drag, type-select, a cross-link) and records the
// selection capsule's on-screen position on every display frame (one layer read per frame), to
// check that every change slides, smoothly, after the new page's first frame (spec §2.3 "Motion
// order"), and that page switches never queue up.
//
// PAGELAMP_PERF_PROBE=segment drives the course section picker's glass thumb (spec §3.2.1) through
// every kind of change (real clicks with a 50 and a 0 ms press, or only PAGELAMP_PERF_PRESS ms;
// down and up in one pass; a held press; a drag; → and Space; VoiceOver's press; the model; Go ▸
// Current Week; a retarget; a cross-link and a second one) and records the thumb's on-screen x on
// every display frame (one layer read per frame), to check that every change slides, from the
// frame after the input, smoothly per unit of time (the render server draws the spring while the
// section builds), and that the selection changes only on mouse-up.

import AppKit
import QuartzCore
import SwiftUI
import PageLampModel

@MainActor
final class PerfProbe: NSObject {
    static let toggleSidebar = Notification.Name("PerfProbe.toggleSidebar")
    /// Gives the sidebar keyboard focus (the capsule mode's keys go to it).
    static let focusSidebar = Notification.Name("PerfProbe.focusSidebar")
    /// The sidebar's capsule host while the capsule mode runs (`register(capsuleHost:)`).
    private static weak var capsuleHost: SidebarCapsuleHostView?
    /// The course section picker while the segment mode runs (`register(segmentedControl:)`; the
    /// latest wins: a cross-link builds a new one).
    private static weak var segmentedControl: GlassSegmentedControl?
    private static let requestedMode = ProcessInfo.processInfo.environment["PAGELAMP_PERF_PROBE"]
    static var longFrames: [[Double]] = []
    private var stamps: [CFTimeInterval] = []
    private var interval: CFTimeInterval = 1.0 / 60.0
    private var link: CADisplayLink?
    /// The capsule mode's per-frame record while a change is measured (nil otherwise).
    private var capsuleSamples: [CapsuleSample]?
    /// The segment mode's per-frame record while a change is measured (nil otherwise).
    private var segmentSamples: [SegmentSample]?
    private var trackedModel: AppModel?

    /// The capsule host registers itself when it enters a window; kept only for the capsule mode.
    static func register(capsuleHost host: SidebarCapsuleHostView) {
        guard requestedMode == "capsule" else { return }
        capsuleHost = host
    }

    /// The section picker registers itself when it enters a window; kept only for the segment mode.
    static func register(segmentedControl control: GlassSegmentedControl) {
        guard requestedMode == "segment" else { return }
        segmentedControl = control
    }

    static func runIfRequested(model: AppModel) async {
        guard let mode = ProcessInfo.processInfo.environment["PAGELAMP_PERF_PROBE"], ["inspector", "navigate", "sidebar", "courses", "enter", "capsule", "segment", "all"].contains(mode) else { return }
        let probe = PerfProbe()
        await probe.run(model: model)
    }

    private var cpuStamps: [(CFTimeInterval, Double)] = []
    /// Main-thread CPU time in ms (not affected by other processes taking the CPU).
    private static func threadCPUms() -> Double { Double(clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)) / 1_000_000 }

    @objc private func tick(_ link: CADisplayLink) {
        stamps.append(link.timestamp)
        cpuStamps.append((CACurrentMediaTime(), Self.threadCPUms()))
        let frame = link.targetTimestamp - link.timestamp
        if frame > 0 { interval = frame }
        // The segment mode: one layer read per frame.
        if segmentSamples != nil, let control = Self.segmentedControl, let layer = control.thumbLayer, let model = trackedModel {
            let read = CACurrentMediaTime()
            // A slide added after this frame began and before this read may not be committed yet.
            let committed = !(control.lastSlideTime > link.timestamp && control.lastSlideTime <= read)
            segmentSamples?.append(SegmentSample(
                frame: link.timestamp, read: read, x: Double((layer.presentation() ?? layer).frame.origin.x),
                committed: committed, control: ObjectIdentifier(control),
                section: model.selectedCourseId.map { model.ui(for: $0).section }
            ))
        }
        // The capsule mode: one layer read per frame (walking the window would distort the timing).
        if capsuleSamples != nil, let host = Self.capsuleHost, let layer = host.capsuleLayer, let model = trackedModel {
            let y = Double((layer.presentation() ?? layer).position.y)
            // A slide added earlier in this same frame isn't committed yet: its read is not what
            // the screen shows (the analysis interpolates it).
            let committed = abs(host.lastSlideFrame - link.timestamp) > 0.001
            capsuleSamples?.append(CapsuleSample(
                frame: link.timestamp, read: CACurrentMediaTime(), y: y, committed: committed, destination: model.pageDestination
            ))
        }
    }

    private func sleep(_ seconds: Double) async {
        try? await Task.sleep(for: .seconds(seconds))
    }

    private func run(model: AppModel) async {
        let env = ProcessInfo.processInfo.environment
        let code = env["PAGELAMP_PERF_COURSE"] ?? "DEMO310"
        let toggles = Int(env["PAGELAMP_PERF_TOGGLES"] ?? "") ?? 10
        // Wait for the fixture data.
        for _ in 0..<100 where model.courses.isEmpty { await sleep(0.1) }
        guard let course = model.courses.first(where: { $0.course.code == code }) ?? model.courses.first else {
            print("PAGELAMP_PERF {\"error\":\"no courses\"}"); NSApp.terminate(nil); return
        }
        model.inspectorShown = false
        model.destination = .course(course.course.id)
        await sleep(2.0)
        guard let view = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil })?.contentView else {
            print("PAGELAMP_PERF {\"error\":\"no window\"}"); NSApp.terminate(nil); return
        }
        NSApp.activate()
        view.window?.makeKeyAndOrderFront(nil)
        let link = view.displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
        await sleep(0.5)

        let mode = env["PAGELAMP_PERF_PROBE"] ?? "all"
        var report: [String: Any] = ["refreshHz": (1 / interval).rounded()]
        if mode == "inspector" || mode == "all" {
            model.destination = .course(course.course.id)
            await sleep(1.0)
            var runs: [[String: Double]] = []
            for _ in 0..<toggles {
                runs.append(await measure { model.inspectorShown.toggle() })  // the toolbar button
            }
            report["inspector"] = Self.summarise(runs)
        }
        if mode == "sidebar" || mode == "all" {
            // PAGELAMP_PERF_PAGE=thisWeek | sources | connect toggles the sidebar over that page instead.
            model.destination = switch env["PAGELAMP_PERF_PAGE"] {
            case "thisWeek": .thisWeek
            case "sources": .sources
            case "connect": .connect
            default: .course(course.course.id)
            }
            model.inspectorShown = false
            await sleep(1.0)
            var runs: [[String: Double]] = []
            for _ in 0..<toggles {
                runs.append(await measure { NotificationCenter.default.post(name: Self.toggleSidebar, object: nil) })
            }
            report["sidebar"] = Self.summarise(runs)
        }
        if mode == "courses" {
            // Course to course: the course page is rebuilt and loads its data each time.
            let ids = model.courses.prefix(3).map { $0.course.id }
            var runs: [[String: Double]] = []
            for i in 0..<(toggles * 2) {
                let id = ids[i % ids.count]
                runs.append(await measure { model.destination = .course(id) })
            }
            report["courses"] = Self.summarise(runs)
        }
        if mode == "enter" {
            // This Week → course → This Week: the course page is built from scratch each time.
            var runs: [[String: Double]] = []
            for _ in 0..<toggles {
                runs.append(await measure { model.destination = .course(course.course.id) })
                _ = await measure { model.destination = .thisWeek }
            }
            report["enter"] = Self.summarise(runs)
        }
        if mode == "capsule" {
            report["capsule"] = await runCapsule(model: model, view: view)
        }
        if mode == "segment" {
            // A course whose current week is known (Go ▸ Current Week works from Deadlines).
            let segmentCourse = env["PAGELAMP_PERF_COURSE"] == nil ? model.courses.first { $0.course.code == "DEMO205" } ?? course : course
            report["segment"] = await runSegment(model: model, view: view, courseId: segmentCourse.course.id, clicks: toggles)
        }
        if mode == "navigate" || mode == "all" {
            model.inspectorShown = false
            let stops: [Destination] = [.thisWeek, .course(course.course.id), .sources, .connect]
            var perStop: [String: [[String: Double]]] = [:]
            for round in 0..<max(toggles, 3) {
                for stop in stops {
                    let r = await measure { model.destination = stop }
                    if round > 0 { perStop[Self.name(stop), default: []].append(r) }  // round 0 = warm-up
                }
            }
            report["navigate"] = perStop.mapValues { Self.summarise($0, skip: 0) }
        }
        link.invalidate()
        report["longFramesAtMs"] = Self.longFrames
        let counts = Self.longFrames.map { Double($0.count) }
        let even = counts.enumerated().filter { $0.offset % 2 == 0 }.map(\.element)
        let odd = counts.enumerated().filter { $0.offset % 2 == 1 }.map(\.element)
        report["longPerToggleEven"] = even.reduce(0, +) / Double(max(even.count, 1))
        report["longPerToggleOdd"] = odd.reduce(0, +) / Double(max(odd.count, 1))
        let data = try? JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
        print("PAGELAMP_PERF " + (data.flatMap { String(data: $0, encoding: .utf8) } ?? "{}"))
        fflush(stdout)
        NSApp.terminate(nil)
    }

    /// One interaction: run `change`, watch frames for 0.9 s.
    private func measure(_ change: () -> Void) async -> [String: Double] {
        stamps.removeAll()
        cpuStamps.removeAll()
        let cpuStart = Self.threadCPUms()
        let start = CACurrentMediaTime()
        change()
        let afterChange = CACurrentMediaTime()
        await sleep(0.9)
        let window = stamps.filter { $0 >= start - 0.001 }
        var gaps: [Double] = []
        if window.count > 1 { for j in 1..<window.count { gaps.append(window[j] - window[j - 1]) } }
        let firstGap = (window.first ?? start) - start
        let hitch = gaps.reduce(0) { $0 + max(0, $1 - interval * 1.5) } + max(0, firstGap - interval * 1.5)
        // Where the long frames are: offsets (ms after the change) of gaps > 1.5 frames
        var longAt: [Double] = []
        if firstGap > interval * 1.5 { longAt.append(0) }
        if window.count > 1 { for j in 1..<window.count where window[j] - window[j - 1] > interval * 1.5 { longAt.append(((window[j - 1] - start) * 1000).rounded()) } }
        Self.longFrames.append(longAt)
        // CPU the main thread spent until the first frame after the change, and over the 0.9 s.
        let cpuFirst = (cpuStamps.first { $0.0 >= start }?.1 ?? cpuStart) - cpuStart
        let cpuTotal = (cpuStamps.last?.1 ?? cpuStart) - cpuStart
        return [
            "cpuFirstMs": cpuFirst,
            "cpuTotalMs": cpuTotal,
            "syncMs": (afterChange - start) * 1000,
            "firstFrameMs": firstGap * 1000,
            "maxGapMs": max(gaps.max() ?? 0, firstGap) * 1000,
            "hitchMsPerS": hitch * 1000 / 0.9,
            "frames": Double(window.count),
        ]
    }

    private static func summarise(_ runs: [[String: Double]], skip: Int = 2) -> [String: Double] {
        let r = runs.dropFirst(skip)
        func mean(_ k: String) -> Double { r.map { $0[k] ?? 0 }.reduce(0, +) / Double(max(r.count, 1)) }
        func worst(_ k: String) -> Double { r.map { $0[k] ?? 0 }.max() ?? 0 }
        func median(_ k: String) -> Double { let v = r.map { $0[k] ?? 0 }.sorted(); return v.isEmpty ? 0 : v[v.count / 2] }
        return ["medCpuFirstMs": median("cpuFirstMs"), "medCpuTotalMs": median("cpuTotalMs"), "medFirstFrameMs": median("firstFrameMs"), "medMaxGapMs": median("maxGapMs"), "medHitchMsPerS": median("hitchMsPerS"),
                "meanHitchMsPerS": mean("hitchMsPerS"), "meanMaxGapMs": mean("maxGapMs"), "worstMaxGapMs": worst("maxGapMs"),
                "meanFirstFrameMs": mean("firstFrameMs"), "meanSyncMs": mean("syncMs"), "n": Double(r.count)]
    }

    private static func name(_ d: Destination) -> String {
        switch d { case .thisWeek: "thisWeek"; case .course: "course"; case .sources: "sources"; case .connect: "connect" }
    }
}

// MARK: - Capsule mode

extension PerfProbe {
    /// One display frame: its timestamp, when it was read, where the capsule was (its layer's
    /// presentation y), whether that read shows committed state, and the destination at that
    /// moment (a change there is a page switch's first frame).
    fileprivate struct CapsuleSample {
        let frame: CFTimeInterval
        let read: CFTimeInterval
        var y: Double
        let committed: Bool
        let destination: Destination
    }

    /// When the capsule mode last posted an input event (the rapid-key and hold rules count from it).
    private static var lastPostedAt: CFTimeInterval = 0

    /// One measured change: an optional unmeasured reset first, then the input.
    private struct CapsuleStep {
        let name: String
        var reset: Destination?
        /// A held key or a drag: the capsule glides before the page (a preview).
        var previews = false
        let input: () async -> Void
    }

    private func runCapsule(model: AppModel, view: NSView) async -> [String: Any] {
        guard let window = view.window else { return ["error": "no window"] }
        model.inspectorShown = false
        model.destination = .thisWeek
        await sleep(1.0)
        NotificationCenter.default.post(name: Self.focusSidebar, object: nil)
        await sleep(0.5)
        guard let host = Self.capsuleHost, let capsule = host.capsuleLayer else { return ["error": "no capsule host"] }
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        await sleep(0.3)
        let activeAtStart = NSApp.isActive
        // Where the capsule and its glass (the first sublayer) are: the glass must fill the capsule.
        let geometry = [capsule.frame, capsule.sublayers?.first?.frame ?? .zero].map {
            [$0.minX, $0.minY, $0.width, $0.height].map { Double($0) }
        }
        // The row height gives the student's Sidebar icon size; the layout gives every row's frame.
        let height = capsule.bounds.height
        let size: SidebarMetrics.Size = height < 28 ? .small : height > 36 ? .large : .medium
        let layout = SidebarLayout(outline: model.sidebarOutline(l10n: model.l10n), metrics: SidebarMetrics(size))
        let courses = model.visibleCourses.map { Destination.course($0.course.id) }
        guard courses.count >= 3 else { return ["error": "needs 3 courses"] }
        let (c1, c2, c3) = (courses[0], courses[1], courses[2])
        func point(_ destination: Destination) -> NSPoint {
            let frame = layout.frame(of: destination) ?? SidebarLayout.Frame(top: 0, height: 0)
            return host.convert(NSPoint(x: host.bounds.midX, y: frame.top + frame.height / 2), to: nil)
        }
        func click(_ destination: Destination, pressMs: Double) async {
            Self.post(mouse: .mouseMoved, at: point(destination), window: window)
            Self.post(mouse: .leftMouseDown, at: point(destination), window: window)
            if pressMs > 0 { await sleep(pressMs / 1000) }
            Self.post(mouse: .leftMouseUp, at: point(destination), window: window)
        }
        func arrow(down: Bool, pause: Double = 0) async {
            Self.post(arrowDown: down, phase: .keyDown, window: window)
            Self.post(arrowDown: down, phase: .keyUp, window: window)
            if pause > 0 { await sleep(pause) }
        }
        let steps: [CapsuleStep] = [
            CapsuleStep(name: "click thisWeek>c1 0ms") { await click(c1, pressMs: 0) },
            CapsuleStep(name: "click c1>c2 50ms") { await click(c2, pressMs: 50) },
            CapsuleStep(name: "click c2>c3 0ms") { await click(c3, pressMs: 0) },
            CapsuleStep(name: "click c3>sources 50ms") { await click(.sources, pressMs: 50) },
            CapsuleStep(name: "click sources>connect 0ms") { await click(.connect, pressMs: 0) },
            CapsuleStep(name: "click connect>thisWeek 50ms") { await click(.thisWeek, pressMs: 50) },
        ] + (0..<5).map { i in CapsuleStep(name: "down \(i + 1)") { await arrow(down: true) } }
            + (0..<5).map { i in CapsuleStep(name: "up \(i + 1)") { await arrow(down: false) } }
            + [("2", 19), ("3", 20), ("1", 18), ("2", 19)].map { key, code in
                CapsuleStep(name: "cmd\(key)") { Self.post(command: key, keyCode: code, window: window) }
            }
            + [
                CapsuleStep(name: "showSource", reset: .thisWeek) {
                    if let failing = model.failingSources.first { model.showSource(failing.id) }
                },
                CapsuleStep(name: "held down", reset: .thisWeek, previews: true) {
                    Self.post(arrowDown: true, phase: .keyDown, window: window)
                    await self.sleep(NSEvent.keyRepeatDelay)
                    for i in 0..<4 {
                        if i > 0 { await self.sleep(NSEvent.keyRepeatInterval) }
                        Self.post(arrowDown: true, phase: .keyDown, repeating: true, window: window)
                    }
                    await self.sleep(NSEvent.keyRepeatInterval)
                    Self.post(arrowDown: true, phase: .keyUp, window: window)
                },
                CapsuleStep(name: "rapid down x3", reset: .thisWeek) {
                    for _ in 0..<3 { await arrow(down: true, pause: 0.04) }
                },
                CapsuleStep(name: "drag c1>c3", reset: .thisWeek, previews: true) {
                    let (from, to) = (point(c1), point(c3))
                    Self.post(mouse: .mouseMoved, at: from, window: window)
                    Self.post(mouse: .leftMouseDown, at: from, window: window)
                    for i in 1...12 {
                        await self.sleep(0.025)
                        let f = Double(i) / 12
                        Self.post(mouse: .leftMouseDragged, at: NSPoint(x: from.x, y: from.y + (to.y - from.y) * f), window: window)
                    }
                    await self.sleep(0.05)
                    Self.post(mouse: .leftMouseUp, at: to, window: window)
                },
                CapsuleStep(name: "type s") { Self.post(character: "s", keyCode: 1, window: window) },
            ]

        trackedModel = model
        var results: [[String: Any]] = []
        for step in steps {
            if let reset = step.reset, model.destination != reset {
                model.destination = reset
                await sleep(0.7)
            }
            let before = model.destination
            capsuleSamples = []
            let start = CACurrentMediaTime()
            Self.lastPostedAt = start
            await step.input()
            let lastEvent = Self.lastPostedAt
            await sleep(0.7)
            let samples = capsuleSamples ?? []
            capsuleSamples = nil
            var result = Self.analyseCapsule(
                samples, start: start, lastEvent: lastEvent, before: before,
                interval: interval, rowHeight: Double(layout.metrics.rowHeight), previews: step.previews
            )
            result["name"] = step.name
            result["end"] = Self.name(model.destination)
            if step.name == "rapid down x3" {
                let ok = model.destination == c3 && (result["pageSwitches"] as? Int ?? 0) <= 2
                    && (result["lastSwitchAfterLastEventMs"] as? Double ?? 0) <= 300
                result["noQueue"] = ok
            }
            if step.name == "held down" { result["noQueue"] = (result["pageSwitches"] as? Int) == 2 }
            results.append(result)
        }
        trackedModel = nil
        let judged = results.filter { $0["judged"] as? Bool == true }
        let passed = judged.filter { $0["pass"] as? Bool == true }
        let passedPerTime = judged.filter { $0["passPerTime"] as? Bool == true }
        let queues = results.compactMap { $0["noQueue"] as? Bool }
        return [
            "changes": results,
            "judged": judged.count,
            "passed": passed.count,
            "passedPerTime": passedPerTime.count,
            "failedPerTime": judged.filter { $0["passPerTime"] as? Bool != true }.compactMap { $0["name"] as? String },
            "failed": judged.filter { $0["pass"] as? Bool != true }.compactMap { $0["name"] as? String },
            "noQueue": queues.allSatisfy { $0 },
            "sidebarSize": "\(size)",
            // Clicks in an inactive app only activate it (macOS may refuse the probe's activation
            // while someone uses another app); keys still reach the key window.
            "appActiveAtStart": activeAtStart,
            "capsuleFrame": geometry[0],
            "glassFrame": geometry[1],
            "sidebarWidth": Double(host.bounds.width),
        ]
    }

    /// The capsule check for one change that moves at least a row: it slides (18 distinct positions
    /// at 120 Hz, 10 at 60 Hz), starts at least a frame after the page's first frame and within
    /// 250 ms of the input, and moves smoothly: no main-thread gap over 1.5 frames, no step over
    /// 1.5× (1.7× at 60 Hz) the median of its ±3 neighbours, the largest step within 1.15× of the
    /// ideal spring's for the same travel. `passPerTime` judges the steps per unit of time instead
    /// (what the render server shows while the main thread is busy).
    private static func analyseCapsule(
        _ samples: [CapsuleSample], start: CFTimeInterval, lastEvent: CFTimeInterval, before: Destination,
        interval: Double, rowHeight: Double, previews: Bool
    ) -> [String: Any] {
        guard samples.count > 2 else { return ["error": "no frames", "judged": false] }
        let is120 = interval < 1.0 / 90
        var samples = samples
        var uncommitted = 0
        for i in samples.indices.dropFirst().dropLast() where !samples[i].committed {
            // A slide that starts from rest hasn't moved on screen in its first frame; a retarget
            // of a moving capsule is halfway between its neighbours.
            let atRest = i < 2 || abs(samples[i - 1].y - samples[i - 2].y) < 0.25
            samples[i].y = atRest ? samples[i - 1].y : (samples[i - 1].y + samples[i + 1].y) / 2
            uncommitted += 1
        }
        var result: [String: Any] = ["frames": samples.count, "uncommittedReads": uncommitted]
        // Page switches: frames whose destination differs from the frame before.
        var pageFrames: [Int] = []
        for (i, sample) in samples.enumerated() where sample.destination != (i == 0 ? before : samples[i - 1].destination) {
            pageFrames.append(i)
        }
        result["pageSwitches"] = pageFrames.count
        if let first = pageFrames.first { result["firstPageFrameMs"] = ((samples[first].read - start) * 1000).rounded() }
        if let last = pageFrames.last { result["lastSwitchAfterLastEventMs"] = ((samples[last].read - lastEvent) * 1000).rounded() }
        // Motion.
        var steps: [(index: Int, size: Double)] = []
        for i in 1..<samples.count {
            let size = abs(samples[i].y - samples[i - 1].y)
            if size >= 0.25 { steps.append((i, size)) }
        }
        var distinct = [samples[0].y]
        for sample in samples where abs(sample.y - (distinct.last ?? sample.y)) > 0.25 { distinct.append(sample.y) }
        let path = steps.map(\.size).reduce(0, +)
        result["distinct"] = distinct.count
        result["rows"] = (path / rowHeight * 10).rounded() / 10
        guard let firstMove = steps.first?.index, let lastMove = steps.last?.index, path >= rowHeight * 0.9 else {
            result["judged"] = false
            return result
        }
        result["judged"] = true
        // The capsule is visibly somewhere new at `firstMove`; it started in the frame before.
        let firstMoveMs = (samples[firstMove].read - start) * 1000
        result["firstMoveMs"] = firstMoveMs.rounded()
        let framesAfterPage = pageFrames.first.map { firstMove - 1 - $0 }
        result["framesAfterPageFrame"] = framesAfterPage ?? -1
        var maxGap = 0.0
        for i in max(firstMove, 1)...lastMove { maxGap = max(maxGap, samples[i].frame - samples[i - 1].frame) }
        result["maxGapFrames"] = (maxGap / interval * 100).rounded() / 100
        let sizes = steps.map(\.size)
        var maxLocal = 0.0
        for j in sizes.indices {
            let neighbours = (max(0, j - 3)...min(sizes.count - 1, j + 3)).filter { $0 != j }.map { sizes[$0] }
            if let median = median(neighbours), median > 0 { maxLocal = max(maxLocal, sizes[j] / median) }
        }
        result["maxLocalRatio"] = (maxLocal * 100).rounded() / 100
        let overMedian = (sizes.max() ?? 0) / (median(sizes) ?? 1)
        let ideal = idealStepRatio(travel: path, interval: interval)
        result["maxOverMedian"] = (overMedian * 100).rounded() / 100
        result["idealMaxOverMedian"] = (ideal * 100).rounded() / 100
        let moves = distinct.count >= (is120 ? 18 : 10)
        let ordered = (framesAfterPage.map { $0 >= 1 } ?? true) && firstMoveMs <= (previews ? 250 + (lastEvent - start) * 1000 : 250)
        let smooth = maxGap <= interval * 1.5 && maxLocal <= (is120 ? 1.5 : 1.7) && overMedian <= ideal * 1.15
        // The slide itself runs in the render server, which keeps drawing it while the main thread
        // is busy: the main-thread reads above see such a gap as one large step. Per unit of time
        // the steps show what the screen shows (a main-thread gap stays visible in maxGapFrames).
        let rates = steps.map { step in
            step.size * interval / max(samples[step.index].read - samples[step.index - 1].read, interval * 0.5)
        }
        var maxLocalRate = 0.0
        for j in rates.indices {
            let neighbours = (max(0, j - 3)...min(rates.count - 1, j + 3)).filter { $0 != j }.map { rates[$0] }
            if let median = median(neighbours), median > 0 { maxLocalRate = max(maxLocalRate, rates[j] / median) }
        }
        let rateOverMedian = (rates.max() ?? 0) / (median(rates) ?? 1)
        result["maxLocalRateRatio"] = (maxLocalRate * 100).rounded() / 100
        result["rateMaxOverMedian"] = (rateOverMedian * 100).rounded() / 100
        let smoothOnScreen = maxLocalRate <= (is120 ? 1.5 : 1.7) && rateOverMedian <= ideal * 1.15
        result["moves"] = moves
        result["ordered"] = ordered
        result["smooth"] = smooth
        result["smoothPerTime"] = smoothOnScreen
        result["pass"] = moves && ordered && smooth
        result["passPerTime"] = moves && ordered && smoothOnScreen
        return result
    }

    private static func median(_ values: [Double]) -> Double? {
        guard !values.isEmpty else { return nil }
        let sorted = values.sorted()
        return sorted.count % 2 == 1 ? sorted[sorted.count / 2] : (sorted[sorted.count / 2 - 1] + sorted[sorted.count / 2]) / 2
    }

    /// Largest step ÷ median step (steps ≥ 0.25 pt) of the ideal spring (`motion.quick`, the
    /// capsule's) over `travel` points, sampled once per display frame: the reference for "smooth".
    private static func idealStepRatio(travel: Double, interval: Double, spring: PLMotion.Spring = PLMotion.quickSpring) -> Double {
        let (duration, bounce) = (spring.duration, spring.bounce)
        let stiffness = pow(2 * Double.pi / duration, 2)
        let damping = 4 * Double.pi * (1 - bounce) / duration
        let w0 = stiffness.squareRoot(), zeta = damping / (2 * w0), wd = w0 * (1 - zeta * zeta).squareRoot()
        func remaining(_ t: Double) -> Double {
            // Bounce 0 (`motion.section`) is critically damped.
            guard zeta < 1 else { return exp(-w0 * t) * (1 + w0 * t) }
            return exp(-zeta * w0 * t) * (cos(wd * t) + zeta * w0 / wd * sin(wd * t))
        }
        var steps: [Double] = []
        var previous = travel
        var t = interval
        while t < 1.5 {
            let y = travel * remaining(t)
            if abs(y - previous) >= 0.25 { steps.append(abs(y - previous)) }
            previous = y
            t += interval
        }
        return (steps.max() ?? 1) / (median(steps) ?? 1)
    }

    // Real input events through the app's event queue (what a mouse or keyboard sends).

    private static func post(mouse type: NSEvent.EventType, at point: NSPoint, window: NSWindow) {
        guard let event = NSEvent.mouseEvent(
            with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
            pressure: type == .leftMouseDown || type == .leftMouseDragged ? 1 : 0
        ) else { return }
        NSApp.postEvent(event, atStart: false)
        lastPostedAt = CACurrentMediaTime()
    }

    private static func post(arrowDown down: Bool, phase: NSEvent.EventType, repeating: Bool = false, window: NSWindow) {
        let key = String(UnicodeScalar(down ? 0xF701 : 0xF700) ?? " ")
        post(key: key, keyCode: down ? 125 : 126, modifiers: [.numericPad, .function], phase: phase, repeating: repeating, window: window)
    }

    private static func post(command key: String, keyCode: UInt16, window: NSWindow) {
        post(key: key, keyCode: keyCode, modifiers: [.command], phase: .keyDown, window: window)
        post(key: key, keyCode: keyCode, modifiers: [.command], phase: .keyUp, window: window)
    }

    private static func post(character key: String, keyCode: UInt16, window: NSWindow) {
        post(key: key, keyCode: keyCode, modifiers: [], phase: .keyDown, window: window)
        post(key: key, keyCode: keyCode, modifiers: [], phase: .keyUp, window: window)
    }

    private static func post(
        key: String, keyCode: UInt16, modifiers: NSEvent.ModifierFlags, phase: NSEvent.EventType,
        repeating: Bool = false, window: NSWindow
    ) {
        guard let event = NSEvent.keyEvent(
            with: phase, location: .zero, modifierFlags: modifiers, timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, characters: key, charactersIgnoringModifiers: key,
            isARepeat: repeating, keyCode: keyCode
        ) else { return }
        NSApp.postEvent(event, atStart: false)
        lastPostedAt = CACurrentMediaTime()
    }
}

// MARK: - Segment mode

extension PerfProbe {
    /// One display frame: its timestamp, when it was read, where the thumb was (its layer's
    /// presentation x), whether that read shows committed state, which control it was (a
    /// cross-link builds a new one) and the course's section at that moment.
    fileprivate struct SegmentSample {
        let frame: CFTimeInterval
        let read: CFTimeInterval
        let x: Double
        let committed: Bool
        let control: ObjectIdentifier
        let section: CourseSection?
    }

    /// How a change should move the thumb.
    private enum SegmentMotion {
        /// One slide from rest: every read must lie on the ideal `motion.section` spring.
        case slide
        /// A slide retargeted mid-way in the same direction: it must never step back.
        case retarget
        /// Following the pointer: no step out of line with its neighbours.
        case follow
        /// Nothing may move (a second cross-link).
        case still
    }

    /// One measured change: an unmeasured reset to a section first, then the input.
    private struct SegmentStep {
        let name: String
        var reset: CourseSection?
        var motion = SegmentMotion.slide
        /// A pointer change: the selection may change only after the (first) mouse-up.
        var pointer = false
        /// A change from the model (the slide is added in the SwiftUI update that also builds the
        /// new section, so it starts with that update's commit, not in the next frame).
        var programmatic = false
        let input: () async -> Void
    }

    private func runSegment(model: AppModel, view: NSView, courseId: String, clicks: Int) async -> [String: Any] {
        guard let window = view.window else { return ["error": "no window"] }
        let ui = model.ui(for: courseId)
        model.inspectorShown = false
        ui.section = .week
        model.destination = .course(courseId)
        await sleep(1.5)
        guard let first = Self.segmentedControl, first.window != nil else { return ["error": "no glass section picker"] }
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        await sleep(0.3)
        let activeAtStart = NSApp.isActive
        /// The control on screen now (a cross-link replaces it).
        func control() -> GlassSegmentedControl? { Self.segmentedControl }
        func point(_ segment: Int) -> NSPoint { control()?.segmentCenter(segment) ?? .zero }
        /// When the step's first mouse-up was posted (the selection may change from then).
        var upPostedAt: CFTimeInterval = 0
        func postUp(at point: NSPoint) {
            if upPostedAt == 0 { upPostedAt = CACurrentMediaTime() }
            Self.post(mouse: .leftMouseUp, at: point, window: window)
        }
        func click(_ segment: Int, pressMs: Double) async {
            Self.post(mouse: .mouseMoved, at: point(segment), window: window)
            Self.post(mouse: .leftMouseDown, at: point(segment), window: window)
            if pressMs > 0 {
                await sleep(pressMs / 1000)
            } else {
                // A trackpad tap: the up follows in the next run-loop turn.
                await Task.yield()
            }
            postUp(at: point(segment))
        }
        func key(_ character: String, code: UInt16) {
            let modifiers: NSEvent.ModifierFlags = character == " " ? [] : [.numericPad, .function]
            Self.post(key: character, keyCode: code, modifiers: modifiers, phase: .keyDown, window: window)
            Self.post(key: character, keyCode: code, modifiers: modifiers, phase: .keyUp, window: window)
        }
        let rightArrow = String(UnicodeScalar(0xF703) ?? " ")
        let env = ProcessInfo.processInfo.environment
        let presses: [Double] = env["PAGELAMP_PERF_PRESS"].flatMap { Double($0) }.map { [$0] } ?? [50, 0]
        let order = [1, 2, 0]
        var steps: [SegmentStep] = []
        for press in presses {
            steps += (0..<clicks).map { i in
                SegmentStep(name: "click \(order[(i + 2) % 3])>\(order[i % 3]) \(Int(press))ms", pointer: true) {
                    await click(order[i % 3], pressMs: press)
                }
            }
        }
        steps += [
            SegmentStep(name: "down+up one pass 0>1", reset: .week, pointer: true) {
                Self.post(mouse: .leftMouseDown, at: point(1), window: window)
                postUp(at: point(1))
            },
            SegmentStep(name: "hold 400ms 0>2", reset: .week, pointer: true) {
                await click(2, pressMs: 400)
            },
            SegmentStep(name: "drag 0>2", reset: .week, motion: .follow, pointer: true) {
                let (from, to) = (point(0), point(2))
                Self.post(mouse: .mouseMoved, at: from, window: window)
                Self.post(mouse: .leftMouseDown, at: from, window: window)
                for i in 1...12 {
                    await self.sleep(0.025)
                    Self.post(mouse: .leftMouseDragged, at: NSPoint(x: from.x + (to.x - from.x) * Double(i) / 12, y: from.y), window: window)
                }
                await self.sleep(0.05)
                postUp(at: to)
            },
            SegmentStep(name: "focus, right, space 0>1", reset: .week) {
                if let control = control() { window.makeFirstResponder(control) }
                key(rightArrow, code: 124)
                await self.sleep(0.1)
                key(" ", code: 49)
            },
            SegmentStep(name: "accessibility press 1>2", reset: .deadlines) {
                window.makeFirstResponder(nil)
                _ = control()?.elements[2].accessibilityPerformPress()
            },
            SegmentStep(name: "model 2>0", reset: .timeline, programmatic: true) { ui.section = .week },
            SegmentStep(name: "current week 1>0", reset: .deadlines, programmatic: true) { model.showCurrentWeek() },
            SegmentStep(name: "retarget 0>1>2", reset: .week, motion: .retarget, pointer: true) {
                await click(1, pressMs: 0)
                await self.sleep(0.08)
                await click(2, pressMs: 0)
            },
            // The picker has shown This Week (pickerSection); This Week then opens the course at
            // Deadlines: the new page's thumb slides from This Week once.
            SegmentStep(name: "cross-link 0>1", reset: .week, programmatic: true) {
                model.destination = .thisWeek
                await self.sleep(0.7)
                ThisWeekNavigation.open(courseId: courseId, section: .deadlines, model: model)
            },
            SegmentStep(name: "second cross-link 1>1", motion: .still) {
                model.destination = .thisWeek
                await self.sleep(0.7)
                ThisWeekNavigation.open(courseId: courseId, section: .deadlines, model: model)
            },
        ]

        trackedModel = model
        var results: [[String: Any]] = []
        for step in steps {
            if let reset = step.reset, ui.section != reset || model.destination != .course(courseId) {
                model.destination = .course(courseId)
                ui.section = reset
                await sleep(0.7)
            }
            let before = ui.section
            // Where the thumb stands before the input (the first read may come only after the
            // section build, already on its way).
            let startX = control().map { Double($0.segmentLayout.thumbFrame(CourseSection.allCases.firstIndex(of: before) ?? 0).minX) }
            let slidesBefore = control()?.lastSlideTime ?? 0
            let controlBefore = control().map(ObjectIdentifier.init)
            upPostedAt = 0
            segmentSamples = []
            let start = CACurrentMediaTime()
            await step.input()
            await sleep(0.7)
            var samples = segmentSamples ?? []
            segmentSamples = nil
            if let startX, let first = samples.first {
                samples.insert(SegmentSample(frame: start, read: start, x: startX, committed: true, control: first.control, section: before), at: 0)
            }
            // The input was handled when its slide was added (a new control: in its first frame).
            let latest = control()
            let handledAt = latest.flatMap { control in
                control.lastSlideTime > slidesBefore || ObjectIdentifier(control) != controlBefore ? control.lastSlideTime : nil
            }
            var result = Self.analyseSegment(
                samples, start: start, handledAt: handledAt, interval: interval, motion: step.motion, programmatic: step.programmatic
            )
            result["name"] = step.name
            result["end"] = ui.section.rawValue
            if env["PAGELAMP_PERF_TRACE"] != nil {
                // Every read: ms after the input, x, committed.
                result["trace"] = samples.map { [(($0.read - start) * 10000).rounded() / 10, ($0.x * 100).rounded() / 100, $0.committed ? 1 : 0] }
            }
            if step.pointer {
                // The selection changes on mouse-up only (never while the press is down).
                let changed = samples.first { $0.section != nil && $0.section != before }
                result["selectionAtMouseUpOnly"] = changed.map { $0.read >= upPostedAt } ?? (ui.section != before)
            }
            results.append(result)
        }
        trackedModel = nil
        return [
            "changes": results,
            "judged": results.count,
            "passed": results.filter { $0["pass"] as? Bool == true && $0["selectionAtMouseUpOnly"] as? Bool != false }.count,
            "failed": results.filter { $0["pass"] as? Bool != true || $0["selectionAtMouseUpOnly"] as? Bool == false }
                .compactMap { $0["name"] as? String },
            // Changes that should slide but jumped or stood still.
            "skipped": results.filter { $0["expectsSlide"] as? Bool == true && ($0["distinct"] as? Int ?? 0) <= 3 }.count,
            "appActiveAtStart": activeAtStart,
            "presses": presses,
        ]
    }

    /// The thumb check for one change (x per display frame of the latest control). Every change
    /// but a `.still` one slides (18 distinct positions at 120 Hz, 10 at 60 Hz; a retarget 14 / 8,
    /// as its window holds two section builds, when the main thread takes no reads) and starts
    /// promptly: a single slide's fitted spring starts at most one frame after the input was handled
    /// (`springLagMs`), a change from the model with the commit of the update that carried it (its
    /// spring has started by the first frame the main thread serves after it, + 1 frame:
    /// `startsWithUpdate`), a retarget or a drag is visibly moving at most one frame after the
    /// input was handled (`firstMoveFrames`). Smoothness, judged per unit of time because the render
    /// server keeps drawing while the section builds: a single slide's reads all lie within 1 pt of
    /// the ideal `motion.section` spring (its start time fitted); a retarget never steps back; a
    /// drag has no step (per unit of time) over 1.5× (1.7× at 60 Hz) the median of its ±3
    /// neighbours. The capsule mode's largest-step ratio (`rateMaxOverMedian`) and the main-thread
    /// gaps while it moves (`mainMaxGapMs`) are reported, not judged: main-thread reads are uneven
    /// (the ideal spring sampled at the same read times fails that ratio just as often).
    ///
    /// Reads are taken as they are: one that may predate its slide's commit (`committed` false: a
    /// late display-link tick can look like one) is left out of the spring fit but never rewritten,
    /// and the fit drops a first displaced read that comes before the spring it fits best without it
    /// (a read during the commit of a new page's first frame).
    private static func analyseSegment(
        _ all: [SegmentSample], start: CFTimeInterval, handledAt: CFTimeInterval?, interval: Double, motion: SegmentMotion,
        programmatic: Bool
    ) -> [String: Any] {
        let expectsSlide = motion != .still
        var result: [String: Any] = ["expectsSlide": expectsSlide]
        // The first sample is where the thumb stood before the input (`runSegment`): a new control
        // (a cross-link) starts there too.
        guard let latest = all.last?.control else {
            result["error"] = "no frames"
            result["pass"] = false
            return result
        }
        let samples = all.enumerated().filter { $0.offset == 0 || $0.element.control == latest }.map(\.element)
        guard samples.count > 2 else {
            result["error"] = "no frames"
            result["pass"] = false
            return result
        }
        let is120 = interval < 1.0 / 90
        // The spring fit uses the reads as taken, less the ones that may predate their slide's commit.
        let reads = samples.filter(\.committed)
        result["frames"] = samples.count
        result["uncommittedReads"] = samples.dropFirst().filter { !$0.committed }.count
        var steps: [(index: Int, size: Double)] = []
        for i in 1..<samples.count {
            let size = samples[i].x - samples[i - 1].x
            if abs(size) >= 0.25 { steps.append((i, size)) }
        }
        var distinct = [samples[0].x]
        for sample in samples where abs(sample.x - (distinct.last ?? sample.x)) > 0.25 { distinct.append(sample.x) }
        let (x0, x1) = (samples[0].x, samples[samples.count - 1].x)
        result["distinct"] = distinct.count
        result["travel"] = ((x1 - x0) * 10).rounded() / 10
        guard let firstMove = steps.first?.index, let lastMove = steps.last?.index else {
            result["slides"] = false
            result["pass"] = !expectsSlide
            return result
        }
        let slides = distinct.count >= (motion == .retarget ? (is120 ? 14 : 8) : (is120 ? 18 : 10))
        result["slides"] = slides
        result["firstMoveMs"] = ((samples[firstMove].read - start) * 1000).rounded()
        var firstMoveFrames: Int?
        var mainFreeAt: CFTimeInterval?
        if let handledAt, let handled = samples.firstIndex(where: { $0.read >= handledAt }) {
            firstMoveFrames = max(firstMove - handled, 0)
            mainFreeAt = samples[handled].read
            result["handledMs"] = ((handledAt - start) * 1000).rounded()
            result["mainFreeMs"] = ((samples[handled].read - start) * 1000).rounded()
        }
        result["firstMoveFrames"] = firstMoveFrames ?? -1
        var maxGap = 0.0
        // Between real reads (the first sample is the start position, not a frame).
        for i in max(firstMove, 2)...max(lastMove, 2) where i < samples.count {
            maxGap = max(maxGap, samples[i].frame - samples[i - 1].frame)
        }
        result["mainMaxGapMs"] = (maxGap * 10000).rounded() / 10
        let sizes = steps.map { abs($0.size) }
        let rates = steps.map { step in
            abs(step.size) * interval / max(samples[step.index].read - samples[step.index - 1].read, interval * 0.5)
        }
        var maxLocalRate = 0.0
        for j in rates.indices {
            let neighbours = (max(0, j - 3)...min(rates.count - 1, j + 3)).filter { $0 != j }.map { rates[$0] }
            if let median = median(neighbours), median > 0 { maxLocalRate = max(maxLocalRate, rates[j] / median) }
        }
        let ideal = idealStepRatio(travel: sizes.reduce(0, +), interval: interval, spring: PLMotion.sectionSpring)
        result["maxLocalRateRatio"] = (maxLocalRate * 100).rounded() / 100
        result["rateMaxOverMedian"] = ((rates.max() ?? 0) / (median(rates) ?? 1) * 100).rounded() / 100
        result["idealMaxOverMedian"] = (ideal * 100).rounded() / 100
        let smooth: Bool
        var prompt = (firstMoveFrames ?? 99) <= 1
        switch motion {
        case .slide:
            let fit = springFit(reads, from: x0, to: x1, spring: PLMotion.sectionSpring)
            result["springStartMs"] = ((fit.start - start) * 1000).rounded()
            result["springMaxErrorPt"] = (fit.maxError * 100).rounded() / 100
            if fit.droppedEarlyRead { result["droppedEarlyRead"] = true }
            smooth = fit.maxError <= 1
            if programmatic, let mainFreeAt {
                let withUpdate = fit.start <= mainFreeAt + interval
                result["startsWithUpdate"] = withUpdate
                prompt = withUpdate
            } else if let handledAt {
                // The spring the reads lie on starts within a frame of the input's handling: a 60 Hz
                // frame at most, so a 120 Hz display doesn't fail a one-frame scheduling hiccup.
                let lag = fit.start - handledAt
                result["springLagMs"] = (lag * 10000).rounded() / 10
                prompt = lag <= max(interval, 1.0 / 60)
            }
        case .retarget:
            let direction = (x1 - x0).sign
            let backward = steps.contains { $0.size.sign != direction }
            result["backward"] = backward
            smooth = !backward
        case .follow:
            smooth = maxLocalRate <= (is120 ? 1.5 : 1.7)
        case .still:
            smooth = false
        }
        result["passPerTime"] = smooth
        result["pass"] = expectsSlide && slides && prompt && smooth
        return result
    }

    /// The ideal spring from `x0` to `x1` that best fits the reads (its start searched in 0.5 ms
    /// steps), and the largest distance of any read from it. A render-server spring lies on it
    /// whatever the main thread did; a thumb stepped by the main thread doesn't. The first read off
    /// `x0` is dropped when the spring fitted without it starts after that read and fits better:
    /// the read was taken before the slide's commit (`droppedEarlyRead`).
    private static func springFit(
        _ samples: [SegmentSample], from x0: Double, to x1: Double, spring: PLMotion.Spring
    ) -> (start: CFTimeInterval, maxError: Double, droppedEarlyRead: Bool) {
        let fit = bestSpring(samples, from: x0, to: x1, spring: spring)
        guard let early = samples.firstIndex(where: { abs($0.x - x0) > 0.25 }) else { return (fit.start, fit.maxError, false) }
        var rest = samples
        rest.remove(at: early)
        let refit = bestSpring(rest, from: x0, to: x1, spring: spring)
        if refit.start > samples[early].read, refit.maxError < fit.maxError { return (refit.start, refit.maxError, true) }
        return (fit.start, fit.maxError, false)
    }

    private static func bestSpring(
        _ samples: [SegmentSample], from x0: Double, to x1: Double, spring: PLMotion.Spring
    ) -> (start: CFTimeInterval, maxError: Double) {
        let w0 = 2 * Double.pi / spring.duration
        let zeta = 1 - spring.bounce
        let wd = w0 * max(1 - zeta * zeta, 0).squareRoot()
        func x(at t: Double, from start: Double) -> Double {
            let tau = t - start
            guard tau > 0 else { return x0 }
            let remaining = zeta < 1
                ? exp(-zeta * w0 * tau) * (cos(wd * tau) + zeta * w0 / wd * sin(wd * tau))
                : exp(-w0 * tau) * (1 + w0 * tau)
            return x1 + (x0 - x1) * remaining
        }
        guard let first = samples.first?.read, let last = samples.last?.read else { return (0, .infinity) }
        var best = (start: first, maxError: Double.infinity)
        var start = first - 0.05
        while start < last {
            let error = samples.map { abs($0.x - x(at: $0.read, from: start)) }.max() ?? .infinity
            if error < best.maxError { best = (start, error) }
            start += 0.0005
        }
        return best
    }
}
