// Review's check (T37.28.2, DT§5.4, DS§6.4): the files by turn with the open file's diff per
// light/dark × Solid/Frosted cell, the same file with nothing left after a code-only rewind, and
// empty.

import Testing

@testable import CoxUI

@MainActor
@Suite struct ReviewPaneSnapshotTests {
  @Test(arguments: Variant.all) func reviewPane(_ variant: Variant) throws {
    try assertCoxSnapshot(
      ReviewPaneSample(state: PreviewState.review), variant, named: variant.name)
  }

  @Test func reviewPaneWithNothingLeft() throws {
    try assertCoxSnapshot(
      ReviewPaneSample(state: PreviewState.reviewNothingLeft), Variant.all[0],
      named: Variant.all[0].name)
  }

  @Test func emptyReviewPane() throws {
    try assertCoxSnapshot(
      ReviewPaneSample(state: .init()), Variant.all[0], named: Variant.all[0].name)
  }
}
