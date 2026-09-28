// `PreviewState` fixture for the Settings search (T37.45.1, mockups 18–20): "mod" typed in the
// sidebar, which keeps the Models & Providers page and its two tiers' models, as CoxModel's
// `SettingsStore.sections` narrows them. Separate so the card adds its fixture without editing
// another card's file.

import SwiftUI

extension PreviewState {
  /// The pages and fields a search for "mod" keeps, each match marked.
  static let settingsFiltered = SettingsScreenState(
    pages: [.models], selection: .models,
    tables: [
      .init(
        id: "tiers.code",
        fields: [
          field(
            "tiers.code.model", .project, .field("claude-sonnet-5"),
            detail: "Set in \(projectFile)")
        ]),
      .init(
        id: "tiers.fast",
        fields: [field("tiers.fast.model", .default, .field("claude-haiku-5"))]),
    ],
    userFile: userFile, projectFile: projectFile, filter: "mod")
}
