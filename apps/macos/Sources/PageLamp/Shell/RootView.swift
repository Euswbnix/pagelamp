// The main window (spec §2.3–§2.4, §3.9 S1/S2): a two-column split view, no back stack.

import AppKit
import SwiftUI
import PageLampModel

public struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    /// Restored per window (spec §2.4): the destination and whether the inspector is open.
    @SceneStorage("destination") private var storedDestination = ""
    @SceneStorage("inspectorShown") private var storedInspector = false
    /// The inspector opens by itself only on the first course visit, on a wide window.
    @AppStorage("inspector.firstCourseVisitDone") private var firstCourseVisitDone = false

    @State private var columns: NavigationSplitViewVisibility = .all
    @State private var width: CGFloat = PLSize.windowMainWidth
    @State private var restored = false

    public init() {}

    public var body: some View {
        @Bindable var model = model
        NavigationSplitView(columnVisibility: $columns) {
            SidebarList()
                .navigationSplitViewColumnWidth(min: PLSize.sidebarMin, ideal: PLSize.sidebarIdeal, max: PLSize.sidebarMax)
        } detail: {
            DetailColumn()
        }
        .frame(minWidth: PLSize.windowMainMinWidth, minHeight: PLSize.windowMainMinHeight)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .diagnosticReportSheet(host: .main)
        .alert(l10n("mac.debug.live.title"), isPresented: $model.confirmingLiveData) {
            Button(l10n("common.actions.cancel"), role: .cancel) {}
                .keyboardShortcut(.defaultAction)
            Button(l10n("mac.debug.live.confirm")) {
                Task { await model.useLive() }
            }
        } message: {
            Text(l10n("mac.debug.live.message"))
        }
        .task {
            restore()
            await model.start()
        }
        .onChange(of: model.phase, initial: true) { _, phase in
            // S2: the whole window explains the problem; the sidebar would only show nothing.
            if case .unavailable = phase {
                columns = .detailOnly
            } else if columns == .detailOnly {
                columns = .all
            }
        }
        .onChange(of: model.destination) { _, destination in
            guard restored else { return }
            storedDestination = Self.encode(destination)
            if case .course = destination, !firstCourseVisitDone {
                firstCourseVisitDone = true
                if width >= PLSize.inspectorOpenAt { model.inspectorShown = true }
            }
        }
        .onChange(of: model.inspectorShown) { _, shown in
            if restored { storedInspector = shown }
        }
        .appAppearance(model.appearance)
    }

    private func restore() {
        guard !restored else { return }
        if let destination = Self.decode(storedDestination) { model.destination = destination }
        model.inspectorShown = storedInspector
        restored = true
    }

    private static func encode(_ destination: Destination) -> String {
        (try? JSONEncoder().encode(destination)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
    }

    private static func decode(_ text: String) -> Destination? {
        guard !text.isEmpty else { return nil }
        return try? JSONDecoder().decode(Destination.self, from: Data(text.utf8))
    }
}

/// The detail column for the current phase and destination. Its width reaches the pages as
/// `detailColumnWidth` (rows with a compact layout read it; the course page measures its own,
/// beside the inspector).
struct DetailColumn: View {
    @Environment(AppModel.self) private var model
    @State private var width: CGFloat?

    var body: some View {
        Group {
            switch model.phase {
            case .loading:
                LoadingState()
            case .unavailable(let failure):
                BackendUnavailableView(failure: failure)
            case .ready:
                switch model.destination {
                case .thisWeek: ThisWeekView()
                case .course(let id): CourseDetailView(courseId: id).id(id)
                case .sources: SourcesView()
                case .connect: ConnectView()
                }
            }
        }
        .environment(\.detailColumnWidth, width)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }
}
