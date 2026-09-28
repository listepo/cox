// `SessionRow` (DS§6.3 row `SessionRow`, the mockup's `.row`): one session in the sidebar — its
// state, title, what it is doing and what it has cost. Separate so every list of sessions
// (sidebar groups, search results) draws a session the same way.

import SwiftUI

/// A `StatusDot` centred on the title line, the title over a subtitle, the cost at the far
/// edge; the selected row sits on `accent.soft`, lifted to e1 (DS§3.4).
struct SessionRow: View {
  /// What the row shows, formatted by the core.
  struct Item: Equatable, Sendable {
    var status: StatusDot.Status
    var title: String
    /// Project and activity, `cox · running cargo nextest`.
    var subtitle: String
    /// The session's cost, `$0.42`, or `nil` for none yet.
    var cost: String?
  }

  let item: Item
  let isSelected: Bool

  init(_ item: Item, isSelected: Bool = false) {
    self.item = item
    self.isSelected = isSelected
  }

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.l, style: .continuous)
    HStack(alignment: .titleLine, spacing: Space.m) {
      StatusDot(item.status)
      VStack(alignment: .leading, spacing: 0) {
        HStack(alignment: .firstTextBaseline, spacing: Space.m) {
          Text(item.title)
            .textStyle(.body)
            .foregroundStyle(Color(.textPrimary))
            .frame(maxWidth: .infinity, alignment: .leading)
          if let cost = item.cost {
            // `text.secondary`, not the mockup's tertiary: a figure must stay readable on
            // frosted glass (DS§8).
            Text(cost)
              .textStyle(.footnote, tabularDigits: true)
              .foregroundStyle(Color(.textSecondary))
          }
        }
        .alignmentGuide(.titleLine) { $0[VerticalAlignment.center] }
        Text(item.subtitle)
          .textStyle(.footnote)
          .foregroundStyle(Color(.textSecondary))
      }
    }
    .lineLimit(1)
    .padding(.horizontal, Space.ml)
    .padding(.vertical, Space.s)
    .background { if isSelected { shape.fill(Color(.accentSoft)) } }
    .elevation(isSelected ? .e1 : .e0, cornerRadius: Radius.l)
    .contentShape(shape)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(isSelected ? .isSelected : [])
  }
}

extension VerticalAlignment {
  /// The middle of a row's title line, where its dot sits.
  private enum TitleLine: AlignmentID {
    static func defaultValue(in dimensions: ViewDimensions) -> CGFloat {
      dimensions[VerticalAlignment.center]
    }
  }

  fileprivate static let titleLine = VerticalAlignment(TitleLine.self)
}

#Preview("selected") {
  PreviewMatrix {
    SessionRow(PreviewState.sessions[0], isSelected: true).frame(width: Size.sidebarWidth)
  }
}
#Preview("list") { PreviewMatrix { SessionList() } }
