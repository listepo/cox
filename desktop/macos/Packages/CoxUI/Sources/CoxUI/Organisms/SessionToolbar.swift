// `SessionToolbar` (DS§6.4 row `SessionToolbar`, the mockup's `.toolbar`; DT§5.1): the open
// session's bar above the transcript — where it lives, its model, mode and cost, Stop while a
// turn runs, the Appearance and inspector buttons with their shortcuts, and the Bypass strip.
// Separate so the window shell shows the session from one value the core fills, and reports what
// the person does as intents.

import SwiftUI

/// `Breadcrumb` at the leading end; the capsules, Stop and the icon buttons at the trailing
/// end, `Size.toolbarHeight` tall on the window's own glass. It owns no session state. While
/// the mode is Bypass a `status.danger` strip runs under the whole bar (DS§3.1, A74).
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

  /// DS§3.1's Bypass strip: thin enough not to crowd the bar, thick enough to see at a glance.
  private static let bypassStripHeight: CGFloat = 3

  var body: some View {
    HStack(spacing: Space.ml) {
      if !isSidebarVisible {
        Spacer().frame(width: Self.windowButtonsWidth)
        ToolbarIconButton(symbol: "sidebar.left", label: "Show sidebar", shortcut: .sidebar) {
          send(.showSidebar)
        }
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
        symbol: "paintbrush", label: "Appearance", shortcut: .appearance,
        isActive: state.popover == .appearance
      ) { send(.open(.appearance)) }
      ToolbarIconButton(
        symbol: "sidebar.right",
        label: isInspectorVisible ? "Hide inspector" : "Show inspector", shortcut: .inspector
      ) { send(.toggleInspector) }
    }
    .padding(.leading, Space.xl)
    .padding(.trailing, Space.l)
    .frame(height: Size.toolbarHeight)
    .overlay(alignment: .bottom) {
      if state.mode == .bypass {
        // Along the panes below: the window's edge while the sidebar is out, a gap in from it.
        Capsule()
          .fill(Color(.statusDanger))
          .frame(height: Self.bypassStripHeight)
          .padding(.leading, isSidebarVisible ? 0 : Size.paneGap)
          .padding(.trailing, Size.paneGap)
          .accessibilityHidden(true)
      }
    }
  }
}

/// A shell toggle's key and the glyphs a tooltip names it by. The sidebar and inspector keys are
/// the defaults of the system `SidebarCommands` (⌃⌘S) and `InspectorCommands` (⌃⌘I), so the
/// app's View menu items and these buttons answer the same keys (A74); Appearance is DS§3.5's.
struct ShellShortcut: Sendable {
  let key: KeyboardShortcut
  let glyphs: String

  static let sidebar = Self(key: .init("s", modifiers: [.control, .command]), glyphs: "⌃⌘S")
  static let inspector = Self(key: .init("i", modifiers: [.control, .command]), glyphs: "⌃⌘I")
  static let appearance = Self(key: .init("a", modifiers: [.command, .option]), glyphs: "⌘⌥A")

  /// The tooltip for a control labelled `label`: its name, then its keys (DS§8).
  func help(_ label: String) -> String { "\(label) (\(glyphs))" }
}

/// A symbol alone on a round `CapsuleStyle` capsule, answering its shortcut, with its name as
/// label and its name and keys as tooltip (DS§8).
private struct ToolbarIconButton: View {
  let symbol: String
  let label: String
  let shortcut: ShellShortcut
  var isActive = false
  let action: () -> Void

  var body: some View {
    Button(action: action) { Image(systemName: symbol).symbolStyle(.transcriptH3) }
      .buttonStyle(CapsuleStyle(isActive ? .active : .plain, isIcon: true))
      .keyboardShortcut(shortcut.key)
      .help(shortcut.help(label))
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
