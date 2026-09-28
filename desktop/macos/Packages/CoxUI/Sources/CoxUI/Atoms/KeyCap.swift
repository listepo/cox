// `KeyCap` (DS§6.2 row `KeyCap`, the mockup's `.kbd`): a keyboard shortcut hint beside a
// control or in a tooltip. Separate so every shortcut is drawn as the same small lifted key.

import SwiftUI

/// The keys on a capsule-glass face with a hairline rim, lifted to e1 (DS§3.4).
struct KeyCap: View {
  /// The shortcut as macOS writes it, `⌘K`.
  let keys: String

  init(_ keys: String) {
    self.keys = keys
  }

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.xs, style: .continuous)
    Text(keys)
      .textStyle(.micro)
      .foregroundStyle(Color(.textTertiary))
      .padding(.horizontal, Space.xs)
      .background { shape.fill(Color(.surfaceCapsule)) }
      .hairline(in: shape)
      .elevation(.e1, cornerRadius: Radius.xs)
  }
}

#Preview { PreviewMatrix { KeyCap(PreviewState.keys) } }
