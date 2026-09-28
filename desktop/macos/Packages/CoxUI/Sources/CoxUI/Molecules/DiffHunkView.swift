// `DiffHunkView` (DS§6.3 row `DiffHunkView`, the mockup's `.diff` with its `.hh`): one hunk of a
// diff — the `@@` header and its lines. Separate from `DiffLineView` so the header and the
// shared gutter width belong to the hunk, and a card or the review pane stacks hunks as units.

import SwiftUI

/// The header in `text.secondary` on `fill.primary`, then the lines on `surface.code`, which
/// holds the readable floor under code (DS§3.5).
struct DiffHunkView: View {
  /// The core's hunk header, `@@ -41,12 +41,26 @@ impl Backoff`.
  let header: String
  let lines: [DiffLineView.Line]
  /// Review's click on a line's number, by its index in `lines` (T37.28.4).
  let comment: (@MainActor (Int) -> Void)?

  init(
    header: String, lines: [DiffLineView.Line], comment: (@MainActor (Int) -> Void)? = nil
  ) {
    self.header = header
    self.lines = lines
    self.comment = comment
  }

  var body: some View {
    let widest = lines.map(\.number).max { $0.count < $1.count }
    VStack(alignment: .leading, spacing: 0) {
      Text(header)
        .textStyle(.monoCode)
        .foregroundStyle(Color(.textSecondary))
        .lineLimit(1)
        .padding(.horizontal, Space.ml)
        .padding(.vertical, Space.xxs)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.fillPrimary))
      ForEach(lines.indices, id: \.self) { index in
        DiffLineView(
          lines[index], widestNumber: widest, comment: comment.map { tap in { tap(index) } })
      }
    }
    .padding(.bottom, Space.s)
    .background(Color(.surfaceCode))
  }
}

#Preview { PreviewMatrix { DiffHunkSample() } }
