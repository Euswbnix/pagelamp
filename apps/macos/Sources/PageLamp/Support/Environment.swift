// Environment values every view reads.

import AppKit
import SwiftUI
import PageLampModel

extension EnvironmentValues {
    /// The strings of the app's current language. The root view sets it from `AppModel.l10n`,
    /// so switching the language re-renders every view (spec §7.3: content switches live).
    @Entry var l10n = L10n(locale: .current, table: StringTable.app)
}

extension View {
    /// Puts the model, its strings and its locale into the environment (root view, Settings,
    /// snapshots).
    package func pageLampEnvironment(_ model: AppModel) -> some View {
        environment(model)
            .environment(\.l10n, model.l10n)
            .environment(\.locale, model.locale)
    }
}

extension View {
    /// Multi-line paragraphs: `.lineSpacing(2)` in Chinese, none otherwise (spec §4.3).
    func paragraphLineSpacing() -> some View {
        modifier(ParagraphLineSpacing())
    }
}

private struct ParagraphLineSpacing: ViewModifier {
    @Environment(\.locale) private var locale

    func body(content: Content) -> some View {
        content.lineSpacing(locale.language.languageCode == .chinese ? PLType.cjkLineSpacing : 0)
    }
}

extension View {
    /// Applies the student's Appearance (Settings ▸ General) to every window (`NSApp.appearance`,
    /// spec §3.5). Both the main window and Settings apply it, so a change made in Settings takes
    /// effect even while the main window is closed.
    func appAppearance(_ appearance: AppAppearance) -> some View {
        onChange(of: appearance, initial: true) { _, appearance in
            NSApp.appearance = switch appearance {
            case .system: nil
            case .light: NSAppearance(named: .aqua)
            case .dark: NSAppearance(named: .darkAqua)
            }
        }
    }
}
