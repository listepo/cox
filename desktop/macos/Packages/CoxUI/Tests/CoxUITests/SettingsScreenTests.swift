// The Settings screen's check (T37.30.1, DT§5.7): the Models & Providers page in every
// light/dark × Solid/Frosted cell — the tier the project's config sets is read-only, names the
// project file and carries the `project` badge, and the provider's key field is empty — the
// Appearance page's controls, and the sidebar, group box and field per cell.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct SettingsScreenSnapshotTests {
  @Test(arguments: Variant.all) func aSettingTheProjectOverridesIsReadOnlyWithItsLayer(
    _ variant: Variant
  ) throws {
    try check(PreviewState.settingsModels, variant)
  }

  @Test func appearancePage() throws {
    try check(PreviewState.settingsAppearance, Variant.all[0])
  }

  @Test(arguments: Variant.all) func settingsSidebar(_ variant: Variant) throws {
    try assertCoxSnapshot(
      SettingsSidebar(
        pages: SettingsPage.allCases, selection: .models, userFile: PreviewState.userFile,
        projectFile: PreviewState.projectFile
      ) { _ in }
      .frame(height: Size.windowMinHeight), variant, named: variant.name)
  }

  @Test(arguments: Variant.all) func settingsGroupBox(_ variant: Variant) throws {
    try assertCoxSnapshot(
      PreviewPane {
        SettingsGroupBox(PreviewState.boxTitle) {
          SettingRowSample.control
          SettingRowSample.readOnly
        }
        .fixedSize()
      }, variant, named: variant.name)
  }

  @Test(arguments: Variant.all) func settingField(_ variant: Variant) throws {
    try assertCoxSnapshot(
      PreviewPane {
        VStack(spacing: Space.m) {
          SettingField(PreviewState.fieldText, prompt: PreviewState.fieldPrompt) { _ in }
          SettingField("", prompt: PreviewState.keyPrompt, isSecure: true) { _ in }
        }
        .fixedSize()
      }, variant, named: variant.name)
  }

  private func check(
    _ state: SettingsScreenState, _ variant: Variant, test: String = #function
  ) throws {
    try assertCoxWindowSnapshot(
      SettingsScreen(state: state) { _ in }, variant,
      size: CGSize(width: Size.windowMinWidth, height: Size.windowMinHeight), testName: test)
  }
}

@Suite struct SettingsScreenTests {
  @Test func pagesFollowTheDesignOrder() {
    #expect(
      SettingsPage.allCases.map(\.title) == [
        "General", "Models & Providers", "Permissions", "Sandbox", "Budget", "MCP Servers",
        "Plugins", "Appearance", "Advanced",
      ])
  }
}
