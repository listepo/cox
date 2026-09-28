// `PreviewState` fixtures for `TerminalPaneChrome` (T51.6): mockup 24's one tab and a second
// opened with `+`, over the mockup's test run drawn as text where the app shows SwiftTerm's
// view. Separate so the organism's fixtures do not edit the shared file.

import SwiftUI

extension PreviewState {
  static let terminalTab = "zsh — wt/retry-jitter"

  static let terminalOneTab = TerminalPaneState(
    tabs: [.init(id: 1, title: terminalTab)], selection: 1)

  static let terminalTwoTabs = TerminalPaneState(
    tabs: [.init(id: 1, title: terminalTab), .init(id: 2, title: terminalTab)], selection: 2)

  /// Mockup 24's `.term` lines.
  static let terminalLines = [
    "➜ retry-jitter git:(wt/retry-jitter) cargo nextest run -p cox-provider-http retry",
    "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.41s",
    "    Starting 6 tests across 1 binary (32 skipped)",
    "        PASS [   0.004s] retry::tests::delay_never_exceeds_cap",
    "     Summary [   0.021s] 6 tests run: 6 passed, 32 skipped",
    "➜ retry-jitter git:(wt/retry-jitter)",
  ]
}

/// The pane across the reading column at the mockup's height, text in the terminal's place.
struct TerminalPaneSample: View {
  let state: TerminalPaneState

  /// Mockup 24's `.term` height.
  static let height: CGFloat = 250

  var body: some View {
    TerminalPaneChrome(state: state) { _ in } content: {
      VStack(alignment: .leading, spacing: 0) {
        ForEach(PreviewState.terminalLines, id: \.self) { Text($0) }
      }
      .textStyle(.monoTerminal)
      .foregroundStyle(Color(.textTerminal))
      .lineLimit(1)
    }
    .frame(width: Size.readingWidth, height: Self.height)
  }
}
