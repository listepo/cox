// The View menu's pane items (DS§4, DT§5.1, A89): Show/Hide Sidebar on ⌃⌘S and Show/Hide
// Inspector on ⌃⌘I, the keys and titles of the system `SidebarCommands` and
// `InspectorCommands`. Those act only on panes the system built, and `MainScreen` lays its panes
// out itself, so these items take the system sidebar group's place and toggle the focused
// window's panes through the actions it publishes.

import CoxUI
import SwiftUI

/// The focused session window's pane toggles and whether each pane is out.
struct ShellActions {
  var isSidebarVisible: Bool
  var isInspectorVisible: Bool
  var toggleSidebar: () -> Void
  var toggleInspector: () -> Void
}

extension FocusedValues {
  @Entry var shell: ShellActions?
}

struct ShellCommands: Commands {
  @FocusedValue(\.shell) private var shell

  var body: some Commands {
    CommandGroup(replacing: .sidebar) {
      Button(shell?.isSidebarVisible == false ? "Show Sidebar" : "Hide Sidebar") {
        shell?.toggleSidebar()
      }
      .keyboardShortcut(ShellShortcut.sidebar.key)
      .disabled(shell == nil)
      Button(shell?.isInspectorVisible == false ? "Show Inspector" : "Hide Inspector") {
        shell?.toggleInspector()
      }
      .keyboardShortcut(ShellShortcut.inspector.key)
      .disabled(shell == nil)
    }
  }
}
