// `LabeledToggle` (DS§6.3 row `LabeledToggle`, the mockup's `.appear .row2` and the Settings
// `.gr` switch rows): an on/off setting named on the left, with an optional line saying what it
// does, and the switch at the far edge. Separate so the Appearance popover and Settings lay out
// a switch the same way.

import SwiftUI

/// The title over an optional `text.secondary` detail, then a `CoxToggleStyle` switch.
struct LabeledToggle: View {
  let title: String
  /// What the setting does, `shows a red strip while on`, or `nil`.
  let detail: String?
  @Binding var isOn: Bool

  init(_ title: String, detail: String? = nil, isOn: Binding<Bool>) {
    self.title = title
    self.detail = detail
    self._isOn = isOn
  }

  var body: some View {
    Toggle(isOn: $isOn) {
      VStack(alignment: .leading, spacing: 0) {
        Text(title)
        if let detail {
          Text(detail)
            .textStyle(.footnote)
            .foregroundStyle(Color(.textSecondary))
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .toggleStyle(CoxToggleStyle())
  }
}

#Preview("on") { PreviewMatrix { LabeledToggleSample(isOn: true, detail: nil) } }
#Preview("off, detail") {
  PreviewMatrix { LabeledToggleSample(isOn: false, detail: PreviewState.toggleDetail) }
}
