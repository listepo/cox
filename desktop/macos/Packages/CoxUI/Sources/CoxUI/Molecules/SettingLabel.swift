// `SettingLabel` (DS§6.3 rows `LabeledToggle` and `SettingRow`, the Settings `.gr .l` with its
// `small`): a setting's name over the line saying what it does. Separate so a switch and any
// other setting control name their setting the same way.

import SwiftUI

/// The title in `font.body` over an optional `text.secondary` detail in `font.footnote`.
struct SettingLabel: View {
  let title: String
  /// What the setting does, `for new sessions`, or `nil`.
  let detail: String?
  /// The row names a thing — a server, a check — rather than a setting: its title semibold, the
  /// mockup's `<b>`.
  let namesItem: Bool

  init(_ title: String, detail: String? = nil, namesItem: Bool = false) {
    self.title = title
    self.detail = detail
    self.namesItem = namesItem
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      Text(title)
        .textStyle(.body)
        .fontWeight(namesItem ? .semibold : nil)
        .foregroundStyle(Color(.textPrimary))
      if let detail {
        Text(detail)
          .textStyle(.footnote)
          .foregroundStyle(Color(.textSecondary))
      }
    }
  }
}
