// swift-tools-version: 6.2
// The native macOS app of PageLamp (SwiftUI, macOS 26+, Apple silicon).
//
// Build the Rust core first; it writes the xcframework and the generated bindings this package
// needs (both git-ignored):
//
//     apps/macos/scripts/build-ffi.sh
//     cd apps/macos && swift build && swift test
//
// The signed preview app (dist/PageLamp Preview.app) comes from apps/macos/scripts/build-app.sh.
//
// Targets, bottom up:
//   PageLampKit        the Rust facade through UniFFI (generated) + bridging helpers
//   PageLampModel      services (live / mock), AppModel, L10n, This Week grouping; no SwiftUI views
//   PageLamp           every view: shell, Chrome/ (the only glass), Components/, Views/; strings + tokens
//   PageLampApp        the @main executable (CFBundleExecutable "PageLampApp", never "pagelamp":
//                      the bundled CLI sidecar is Contents/MacOS/pagelamp and APFS is case-insensitive)
//   PageLampSnapshots  renders content views to PNGs with ImageRenderer (headless review)
import PackageDescription

/// Every Swift target: Swift 6 language mode (package-wide) and no warnings.
let strict: [SwiftSetting] = [.treatAllWarnings(as: .error)]

/// UI targets: views run on the main actor anyway. PAGELAMP_PREVIEW compiles in the Debug menu
/// (mock/live data switch); a student build drops the define.
let ui: [SwiftSetting] = strict + [
    .defaultIsolation(MainActor.self),
    .define("PAGELAMP_PREVIEW"),
]

let package = Package(
    name: "PageLamp",
    defaultLocalization: "en",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "PageLampKit", targets: ["PageLampKit"]),
        .executable(name: "PageLampApp", targets: ["PageLampApp"]),
        .executable(name: "PageLampSnapshots", targets: ["PageLampSnapshots"]),
    ],
    targets: [
        // libpagelamp_ffi.a (crates/pagelamp-ffi) + its C header and module map.
        .binaryTarget(
            name: "pagelamp_ffiFFI",
            path: "Frameworks/PageLampFFI.xcframework"
        ),
        // The Rust facade in Swift: the UniFFI bindings (Sources/PageLampKit/Generated) plus
        // small bridging helpers. Keeps the DEFAULT (nonisolated) isolation on purpose: the
        // generated code does not compile under MainActor-by-default (mozilla/uniffi-rs#2818).
        .target(
            name: "PageLampKit",
            dependencies: ["pagelamp_ffiFFI"],
            swiftSettings: strict,
            linkerSettings: [
                // What the static library needs (`--print native-static-libs`), plus
                // SystemConfiguration for network-configuration lookups of the HTTP stack.
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("SystemConfiguration"),
                .linkedLibrary("iconv"),
            ]
        ),
        // App state and data access, testable without any view. Nonisolated by default: the
        // services are Sendable (MockService is an actor); AppModel is explicitly @MainActor.
        .target(
            name: "PageLampModel",
            dependencies: ["PageLampKit"],
            swiftSettings: strict
        ),
        // All views. Resources = the generated en / zh-Hans .lproj (scripts/gen-strings.mjs);
        // Generated/ also holds PLTokens.swift (design/tokens) and L10nKeys.swift.
        .target(
            name: "PageLamp",
            dependencies: ["PageLampModel", "PageLampKit"],
            resources: [.process("Resources")],
            swiftSettings: ui
        ),
        .executableTarget(
            name: "PageLampApp",
            dependencies: ["PageLamp", "PageLampModel"],
            swiftSettings: ui
        ),
        .executableTarget(
            name: "PageLampSnapshots",
            dependencies: ["PageLamp", "PageLampModel"],
            swiftSettings: ui
        ),
        .testTarget(
            name: "PageLampKitTests",
            dependencies: ["PageLampKit"],
            swiftSettings: strict
        ),
        .testTarget(
            name: "PageLampModelTests",
            dependencies: ["PageLampModel", "PageLamp", "PageLampKit"],
            swiftSettings: strict
        ),
    ],
    swiftLanguageModes: [.v6]
)
