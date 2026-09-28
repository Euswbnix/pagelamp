// Captures SwiftUI's own spring values for the motion tokens (fixture for gen-tokens.test.mjs).
// Headless; run on a Mac with Xcode:
//   xcrun swift design/tokens/test/swiftui-springs.swift > design/tokens/test/fixtures/swiftui-springs.json
import Foundation
import SwiftUI

let presets: [(String, Spring)] = [
    ("calm", .smooth(duration: 0.45)),
    ("quick", .snappy(duration: 0.3)),
    ("lively", .bouncy),
    ("week", .smooth(duration: 0.35)),
    ("section", .smooth(duration: 0.25)),
    ("step", .smooth(duration: 0.4)),
]
var out: [[String: Any]] = []
for (token, spring) in presets {
    let samples = (0...20).map { spring.value(target: 1.0, time: Double($0) * 0.05) }
    out.append([
        "token": "motion.\(token)",
        "duration": spring.duration,
        "bounce": spring.bounce,
        "mass": spring.mass,
        "stiffness": spring.stiffness,
        "damping": spring.damping,
        "settlingDuration": spring.settlingDuration,
        "samplesEvery50ms": samples,
    ])
}
let json = try JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: json, as: UTF8.self))
