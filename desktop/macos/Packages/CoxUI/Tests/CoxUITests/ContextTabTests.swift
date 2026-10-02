// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

// The Context tab's check (T37.29.3.1, DT§5.1, DS§6.4): the inspector on its Context tab per
// light/dark × Solid/Frosted cell, with the window unknown, with the cost by turn (T37.29.3.2),
// with the session's cache hit while a turn runs (A104, A105), and empty.

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

  @Test(arguments: Variant.all) func contextTabWithCosts(_ variant: Variant) throws {
    try assertCoxSnapshot(
      ContextInspectorSample(state: PreviewState.contextCosts), variant, named: variant.name)
  }

  /// A104, A105: the session's cache hit, and "Compact now" disabled while the turn runs.
  @Test(arguments: Variant.all) func contextTabWhileATurnRuns(_ variant: Variant) throws {
    try assertCoxSnapshot(
      ContextInspectorSample(state: PreviewState.contextRunning), variant, named: variant.name)
  }

  @Test func emptyContextTab() throws {
    try assertCoxSnapshot(
      ContextInspectorSample(state: .init()), Variant.all[0], named: Variant.all[0].name)
  }
}
