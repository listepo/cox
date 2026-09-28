// `CountBadge` (DS§6.2 row `CountBadge`, the mockup's `.sect .cnt`): how many things wait for
// you, beside a section header. Separate from `Badge` because it is a solid, lifted pill that
// asks to be looked at, not a quiet tag.

import SwiftUI

/// White tabular digits on a `status.warning` pill at e1, at least as wide as it is tall.
struct CountBadge: View {
  /// The count as Rust formats it ("3", "99+").
  let count: String

  /// The mockup's `line-height: 16px`: no size token is that small.
  private static let height: CGFloat = 16
  /// The mockup's `color: #fff`: white on the warning colour in both appearances.
  private static let onWarning = Color.white

  init(_ count: String) {
    self.count = count
  }

  var body: some View {
    Text(count)
      .textStyle(.micro, tabularDigits: true)
      .foregroundStyle(Self.onWarning)
      .padding(.horizontal, Space.s)
      .frame(minWidth: Self.height, minHeight: Self.height)
      .background(Color(.statusWarning), in: Capsule())
      .elevation(.e1, cornerRadius: Self.height / 2)
  }
}

#Preview { PreviewMatrix { CountBadge(PreviewState.count) } }
