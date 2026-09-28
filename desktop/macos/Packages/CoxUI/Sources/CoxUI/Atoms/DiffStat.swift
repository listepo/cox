// `DiffStat` (DS§6.2 row `DiffStat(added, removed)`, the mockup's `.plus` and `.minus`): the
// lines a change adds and removes, "+42 −7". Separate so a tool row, a turn summary and the
// changes tab count lines the same way.

import SwiftUI

/// Tabular `+added` in `status.success` and `−removed` in `status.danger`.
struct DiffStat: View {
  let added: Int
  let removed: Int

  var body: some View {
    HStack(spacing: Space.xs) {
      Text(verbatim: "+\(added)").foregroundStyle(Color(.statusSuccess))
      Text(verbatim: "−\(removed)").foregroundStyle(Color(.statusDanger))
    }
    .textStyle(.footnote, tabularDigits: true)
    .fontWeight(.semibold)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(label)
  }

  var label: String { "\(added) lines added, \(removed) removed" }
}

#Preview { PreviewMatrix { DiffStat(added: PreviewState.added, removed: PreviewState.removed) } }
