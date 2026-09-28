// The Context tab's check (T37.29.3.1, DT§5.1, DS§6.4): the inspector on its Context tab per
// light/dark × Solid/Frosted cell, with the window unknown, and empty.

import Testing

@testable import CoxUI

@MainActor
@Suite struct ContextTabSnapshotTests {
  @Test(arguments: Variant.all) func contextTab(_ variant: Variant) throws {
    try assertCoxSnapshot(
      ContextInspectorSample(state: PreviewState.contextTab), variant, named: variant.name)
  }

  @Test func contextTabWithoutAWindow() throws {
    try assertCoxSnapshot(
      ContextInspectorSample(state: PreviewState.contextNoWindow), Variant.all[0],
      named: Variant.all[0].name)
  }

  @Test func emptyContextTab() throws {
    try assertCoxSnapshot(
      ContextInspectorSample(state: .init()), Variant.all[0], named: Variant.all[0].name)
  }
}
