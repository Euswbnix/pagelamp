// Performance probe (a debug tool; does nothing unless PAGELAMP_PERF_PROBE is set).
//
// PAGELAMP_PERF_PROBE=inspector | sidebar | navigate | courses | enter | all opens a course and
// repeats the interaction exactly as the UI does it (the inspector button, the sidebar toggle,
// page switches), records every display frame with the window's display link, prints one JSON
// line (median first-frame delay, longest frame, hitch ms per second, main-thread CPU time, where
// the long frames were) and quits. Panel-animation jank has no unit-test seam (it lives in
// AppKit's window layout), so run scripts/perf-probe.sh on a release build before and after
// layout changes.

import AppKit
import QuartzCore
import SwiftUI
import PageLampModel

@MainActor
final class PerfProbe: NSObject {
    static let toggleSidebar = Notification.Name("PerfProbe.toggleSidebar")
    static var longFrames: [[Double]] = []
    private var stamps: [CFTimeInterval] = []
    private var interval: CFTimeInterval = 1.0 / 60.0
    private var link: CADisplayLink?

    static func runIfRequested(model: AppModel) async {
        guard let mode = ProcessInfo.processInfo.environment["PAGELAMP_PERF_PROBE"], ["inspector", "navigate", "sidebar", "courses", "enter", "all"].contains(mode) else { return }
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
