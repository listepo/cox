// The first-run window's check (T37.31, DT§5.8): with no provider key the checklist says what is
// missing and offers Settings; with every check passing it offers nothing but the project step.
// Both in every light/dark × Solid/Frosted cell, and the row in each status.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct OnboardingScreenSnapshotTests {
  @Test(arguments: Variant.all) func noProvider(_ variant: Variant) throws {
    try check(PreviewState.onboardingNoProvider, variant)
  }

  @Test(arguments: Variant.all) func allGreen(_ variant: Variant) throws {
    try check(PreviewState.onboardingAllGreen, variant)
  }

  @Test(arguments: Variant.all) func checklistRows(_ variant: Variant) throws {
    try assertCoxSnapshot(
      PreviewPane {
        SettingsGroupBox(PreviewState.checksTitle) {
          ForEach(ChecklistRow.Status.allCases, id: \.self) { ChecklistRowSample(status: $0) }
        }
        .fixedSize()
      }, variant, named: variant.name)
  }

  private func check(
    _ state: OnboardingScreenState, _ variant: Variant, test: String = #function
  ) throws {
    try assertCoxWindowSnapshot(
      OnboardingScreen(state: state) { _ in }, variant,
      size: CGSize(width: Size.windowMinWidth, height: Size.windowMinHeight), testName: test)
  }
}

@Suite struct OnboardingScreenTests {
  @Test func eachFixReportsItsIntent() {
    #expect(OnboardingScreen.Fix.openSettings.intent == .openSettings)
    #expect(OnboardingScreen.Fix.retry.intent == .retry)
  }
}
