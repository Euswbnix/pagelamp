// Preview-only switches (the Debug menu, PAGELAMP_PREVIEW builds). Nothing else sets them, so a
// release build always runs the defaults.

enum DebugPreferences {
    /// The sidebar capsule's timing, to compare on device (spec §2.3 "Motion order"):
    /// false (default): the page switches first and the capsule slides once it is drawn;
    /// true: the capsule starts in the frame of the choice and the page follows two display
    /// frames later (the render server keeps the slide moving while the main thread builds it).
    static let sidebarCapsuleLeads = "debug.sidebarCapsuleLeads"
    /// Course pages show the system section picker (the tabs control of before the glass thumb,
    /// spec §3.2.1) instead of the custom one, to compare on device.
    static let systemSectionPicker = "debug.systemSectionPicker"
}
