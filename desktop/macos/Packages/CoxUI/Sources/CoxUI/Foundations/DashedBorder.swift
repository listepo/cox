// `.dashedBorder(in:color:)` (DS§6.1): the mockup's `border:1.5px dashed` — the outline of a place
// something can be dropped (the first-run window's project drop zone, T37.45.5). Separate so every
// drop target has one line width and one dash, spelled out here as only Foundations may (DS§9).

import SwiftUI

extension View {
  /// Draws a dashed border just inside `shape`, in `color`.
  func dashedBorder(in shape: some InsettableShape, color: Color) -> some View {
    overlay {
      shape
        .strokeBorder(
          color, style: StrokeStyle(lineWidth: DashedBorder.lineWidth, dash: DashedBorder.dash)
        )
        .allowsHitTesting(false)
    }
  }
}

enum DashedBorder {
  /// The mockup's `1.5px`: three hairlines, so the dashes read on frosted glass.
  static let lineWidth: CGFloat = 1.5
  /// Dash and gap, on the spacing steps so the pattern keeps the grid's rhythm.
  static let dash: [CGFloat] = [Space.s, Space.xs]
}
