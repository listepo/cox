// `DiffLineView` (DS§6.3 row `DiffLineView`, the mockup's `.diff .ln`): one line of a diff — its
// line number in a gutter, its sign and its highlighted code on the added, removed or context
// background. Separate so a tool card, the review pane and the changes tab draw a line the same
// way; `DiffHunkView` stacks them.

import SwiftUI

/// The gutter number right-aligned in `text.secondary` (`text.primary` on a tinted gutter; the
/// mockup's tertiary would miss DS§8's 4.5:1 on a light code surface),
/// then `+`, `-` or a space and the code in `font.mono.code`, cut with an ellipsis.
struct DiffLineView: View {
  enum Kind: CaseIterable, Sendable {
    case context, added, removed
  }

  struct Line: Equatable, Sendable {
    var kind: Kind
    /// The line number the core picked (new for added and context, old for removed), `42`.
    var number: String
    var runs: [CodeRun]
  }

  let line: Line
  /// The widest number in the hunk, so every gutter in it has one width at any text size.
  let widestNumber: String

  init(_ line: Line, widestNumber: String? = nil) {
    self.line = line
    self.widestNumber = widestNumber ?? line.number
  }

  var body: some View {
    HStack(spacing: 0) {
      ZStack(alignment: .trailing) {
        Text(widestNumber).hidden()
        Text(line.number)
      }
      .foregroundStyle(line.kind == .context ? Color(.textSecondary) : Color(.textPrimary))
      .padding(.leading, Space.m)
      .padding(.trailing, Space.ml)
      .frame(maxHeight: .infinity)
      .background(line.kind.gutter)
      Text(CodeRun.attributed([[CodeRun("\(line.kind.sign) ")] + line.runs]))
        .foregroundStyle(Color(.textPrimary))
        .padding(.leading, Space.m)
        // `font.mono.code`'s 1.55 line height, which one-line text does not get from
        // `lineSpacing`; the gutter stretches to match.
        .padding(.vertical, Space.xxs)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .textStyle(.monoCode)
    .lineLimit(1)
    .background(line.kind.background)
    .fixedSize(horizontal: false, vertical: true)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel("\(line.kind.label) \(line.number): \(line.runs.map(\.text).joined())")
  }
}

extension DiffLineView.Kind {
  var sign: String {
    switch self {
    case .context: " "
    case .added: "+"
    case .removed: "-"
    }
  }

  var background: Color {
    switch self {
    case .context: .clear
    case .added: Color(.diffAdd)
    case .removed: Color(.diffDel)
    }
  }

  var gutter: Color {
    switch self {
    case .context: .clear
    case .added: Color(.diffAddGutter)
    case .removed: Color(.diffDelGutter)
    }
  }

  var label: String {
    switch self {
    case .context: "Line"
    case .added: "Added line"
    case .removed: "Removed line"
    }
  }
}

#Preview("context") { PreviewMatrix { DiffLineSample(kind: .context) } }
#Preview("added") { PreviewMatrix { DiffLineSample(kind: .added) } }
#Preview("removed") { PreviewMatrix { DiffLineSample(kind: .removed) } }
