// EmptyState (spec §4.5): one SF Symbol at 44 pt in a ContentUnavailableView. Errors never glow.

import SwiftUI

struct EmptyState<Actions: View>: View {
    var symbol: String
    var title: String
    var message: String?
    @ViewBuilder var actions: Actions

    init(symbol: String, title: String, message: String? = nil, @ViewBuilder actions: () -> Actions) {
        self.symbol = symbol
        self.title = title
        self.message = message
        self.actions = actions()
    }

    var body: some View {
        ContentUnavailableView {
            Label {
                Text(title)
            } icon: {
                Image(systemName: symbol)
                    .font(.system(size: PLSize.glyphEmpty))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(.tertiary)
            }
        } description: {
            if let message {
                Text(message)
                    .paragraphLineSpacing()
            }
        } actions: {
            actions
        }
        .frame(maxWidth: .infinity)
    }
}

extension EmptyState where Actions == EmptyView {
    init(symbol: String, title: String, message: String? = nil) {
        self.init(symbol: symbol, title: title, message: message) { EmptyView() }
    }
}
