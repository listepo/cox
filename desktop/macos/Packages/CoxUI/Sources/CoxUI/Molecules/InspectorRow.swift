// `InspectorRow` (DS§6.3 row `ChangedFileRow`, `CheckpointRow`, the mockup's `.fr`): the line
// the inspector lists things on — a glyph, what the row names, a trailing figure and the row's
// actions. Separate so a changed file and a checkpoint share one layout, selection and action
// strip instead of each drawing its own.

import SwiftUI

/// Something a row lets you do to its item, `Revert` or `Rewind`.
struct RowAction: Identifiable, Sendable {
  let title: String
  /// The DS§3.7 symbol of the action's icon button.
  let symbol: String
  let perform: @MainActor () -> Void

  var id: String { title }
}

/// The glyph, then `content` (the name and its trailing figure), then the actions as icon
/// buttons while the row is hovered or selected. The selected row sits on `accent.soft`, lifted
/// to e1 like a `SessionRow`; VoiceOver reads the row as one element with the actions attached.
struct InspectorRow<Content: View>: View {
  let symbol: String
  let isSelected: Bool
  let actions: [RowAction]
  @ViewBuilder let content: Content
  @State private var isHovered = false

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.l, style: .continuous)
    HStack(spacing: Space.m) {
      Image(systemName: symbol).symbolStyle(.body)
      content
      if isHovered || isSelected {
        ForEach(actions) { RowActionButton(action: $0) }
      }
    }
    .textStyle(.body)
    .foregroundStyle(Color(.textPrimary))
    .lineLimit(1)
    .padding(.horizontal, Space.m)
    .padding(.vertical, Space.s)
    .background { if isSelected { shape.fill(Color(.accentSoft)) } }
    .elevation(isSelected ? .e1 : .e0, cornerRadius: Radius.l)
    .contentShape(shape)
    .onHover { isHovered = $0 }
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(isSelected ? .isSelected : [])
    .accessibilityActions {
      ForEach(actions) { action in Button(action.title, action: action.perform) }
    }
  }
}

/// An action's glyph as a bare button, named by its tooltip (DS§8); a line high, so showing
/// the strip does not change the row's height.
private struct RowActionButton: View {
  let action: RowAction

  var body: some View {
    Button(action: action.perform) {
      Image(systemName: action.symbol).symbolStyle(.footnote)
    }
    .buttonStyle(.plain)
    .foregroundStyle(Color(.textSecondary))
    .help(action.title)
    .accessibilityHidden(true)
  }
}
