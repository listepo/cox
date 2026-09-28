import SwiftUI

/// Under `Foundations/` a modifier may build shadows and timings from literals.
struct Elevation: ViewModifier {
  func body(content: Content) -> some View {
    content
      .shadow(color: Color.accentFill, radius: 4, y: 2)
      .animation(.easeOut(duration: 0.2), value: 0)
  }
}
