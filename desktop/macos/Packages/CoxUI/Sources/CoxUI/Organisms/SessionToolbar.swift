// `SessionToolbar` (DS§6.4 row `SessionToolbar`, the mockup's `.toolbar`; DT§5.1): the open
// session's bar above the transcript — where it lives, its model, mode and cost, Stop while a
// turn runs, and the Appearance and inspector buttons. Separate so the window shell shows the
// session from one value the core fills, and reports what the person does as intents.

import SwiftUI

/// `Breadcrumb` at the leading end; the capsules, Stop and the icon buttons at the trailing
/// end, `Size.toolbarHeight` tall on the window's own glass. It owns no session state.
struct SessionToolbar: View {
  /// The popover a capsule or button opens; the one open marks its capsule active.
  enum Popover: Equatable, Sendable {
    case model, cost, appearance
  }

  struct State: Equatable, Sendable {
    var title = ""
    var project = ""
    var branch: String?
    /// The model and effort as the core formats them, `Sonnet 5 · high`.
    var model = ""
    var mode = ModeSegmented.Mode.ask
    /// What the core formatted: `$0.42` and `38%`, and the share the ring fills.
    var cost = ""
    var context = ""
    var contextFraction = 0.0
    var isRunning = false
    var popover: Popover?
  }

  enum Intent: Equatable, Sendable {
    case showSidebar
    case open(Popover)
    case mode(ModeSegmented.Mode)
    case stop
    case toggleInspector
  }

  let state: State
  /// The window's layout, which the screen owns: with the sidebar hidden its toggle moves here.
  let isSidebarVisible: Bool
  let isInspectorVisible: Bool
  let send: (Intent) -> Void

  init(
    state: State, isSidebarVisible: Bool = true, isInspectorVisible: Bool = true,
    send: @escaping (Intent) -> Void
  ) {
    self.state = state
    self.isSidebarVisible = isSidebarVisible
    self.isInspectorVisible = isInspectorVisible
    self.send = send
  }

  /// The system's close, minimise and zoom buttons at the window's leading edge, which the
  /// bar leaves clear while the sidebar that otherwise holds them is hidden: the mockup's
  /// `.traffic` row, 16 pt inset plus three 12 pt buttons 8 pt apart.
  private static let windowButtonsWidth: CGFloat = 68

  var body: some View {
    HStack(spacing: Space.ml) {
      if !isSidebarVisible {
        Spacer().frame(width: Self.windowButtonsWidth)
        ToolbarIconButton(symbol: "sidebar.left", label: "Show sidebar") { send(.showSidebar) }
      }
      Breadcrumb(state.title, project: state.project, branch: state.branch)
      Spacer(minLength: Space.ml)
      ModelCapsule(state.model, isOpen: state.popover == .model) { send(.open(.model)) }
      ModeSegmented(selection: Binding(get: { state.mode }, set: { send(.mode($0)) }))
      CostCapsule(
        cost: state.cost, context: state.context, fraction: state.contextFraction,
        isOpen: state.popover == .cost
      ) { send(.open(.cost)) }
      if state.isRunning { StopButton { send(.stop) } }
      ToolbarIconButton(
        symbol: "paintbrush", label: "Appearance", isActive: state.popover == .appearance
      ) { send(.open(.appearance)) }
      ToolbarIconButton(
        symbol: "sidebar.right",
        label: isInspectorVisible ? "Hide inspector" : "Show inspector"
      ) { send(.toggleInspector) }
    }
    .padding(.leading, Space.xl)
    .padding(.trailing, Space.l)
    .frame(height: Size.toolbarHeight)
  }
}

/// A symbol alone on a round `CapsuleStyle` capsule, with its name as tooltip and label (DS§8).
private struct ToolbarIconButton: View {
  let symbol: String
  let label: String
  var isActive = false
  let action: () -> Void

  var body: some View {
    Button(action: action) { Image(systemName: symbol).symbolStyle(.transcriptH3) }
      .buttonStyle(CapsuleStyle(isActive ? .active : .plain, isIcon: true))
      .help(label)
      .accessibilityLabel(label)
  }
}

#Preview("running") {
  PreviewMatrix { SessionToolbar(state: PreviewState.toolbar) { _ in }.fixedSize() }
}
#Preview("sidebar hidden") {
  PreviewMatrix {
    SessionToolbar(state: PreviewState.toolbar, isSidebarVisible: false) { _ in }.fixedSize()
  }
}
