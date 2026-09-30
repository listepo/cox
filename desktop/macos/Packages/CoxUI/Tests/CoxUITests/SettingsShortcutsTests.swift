// The General page's shortcuts (T51.15, DT§4.6): "Show cox menu" and "New session" with their
// recorders, unbound, in every light/dark × Solid/Frosted cell. The recorder is a stand-in for
// the app's hotkey library, which CoxUI does not link.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct SettingsShortcutsSnapshotTests {
  @Test(arguments: Variant.all) func theGeneralPageShowsBothRecorders(_ variant: Variant) throws {
    try assertCoxWindowSnapshot(
      SettingsScreen(
        state: PreviewState.settingsGeneral, recorder: PreviewState.shortcutRecorder,
        send: { _ in }),
      variant, size: CGSize(width: Size.windowMinWidth, height: Size.windowMinHeight))
  }
}
