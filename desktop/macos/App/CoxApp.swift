// The app target's entry (DT§4.6, DT§7, A106): `@main` and its scenes. Thin by design — every
// view and store lives in the local packages; this target picks the core at launch and hosts
// the window. The window chrome, the Settings scene and the View menu's pane commands are
// T37.22.3's; the menus are the system's until then.

import CoxClient
import SwiftUI

@main
struct CoxApp: App {
  /// Picked once per launch, so every window opens its session on the same core.
  private let core = LaunchCore.pick()

  var body: some Scene {
    WindowGroup("Cox") {
      SessionWindow(core: core)
    }
  }
}
