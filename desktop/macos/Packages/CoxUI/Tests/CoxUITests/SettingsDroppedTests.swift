// Dropped project values (T37.30.4, DT§5.7): a project file that raises the session budget, the
// value listed as dropped with its reason on the Budget page, in every light/dark ×
// Solid/Frosted cell.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct SettingsDroppedSnapshotTests {
  @Test(arguments: Variant.all)
  func aRaisedBudgetIsListedAsDroppedWithItsReason(_ variant: Variant) throws {
    try assertCoxWindowSnapshot(
      SettingsScreen(state: PreviewState.settingsBudgetDropped) { _ in }, variant,
      size: CGSize(width: Size.windowMinWidth, height: Size.windowMinHeight))
  }
}
