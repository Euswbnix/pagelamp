// The facade's limits on a study plan request (StudyPlanRequest; design §5.1).

import Foundation

/// The same numbers as the Tauri app's PLAN_LIMITS, until the facade's `planLimits()` (coming)
/// replaces this copy.
public enum StudyPlanLimits {
    public static let minHorizonDays: UInt32 = 1
    public static let maxHorizonDays: UInt32 = 56
    public static let defaultHorizonDays: UInt32 = 14
    public static let minHoursPerWeek: UInt32 = 1
    public static let maxHoursPerWeek: UInt32 = 80
    public static let defaultHoursPerWeek: UInt32 = 10
    public static let noteMaxChars = 500
}
