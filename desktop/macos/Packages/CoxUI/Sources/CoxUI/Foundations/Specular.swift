// `.specular(_:in:)` (DS§3.5, DS§6.1): the diagonal sweep and streak that make glass read as
// glass, the mockup's `.window:after`. Separate so the sweep's shape lives in one place; it
// draws nothing when the effective material is Solid, so Reduce Transparency removes it too.

import SwiftUI

extension View {
  /// Overlays the highlight at `strength` (a `MaterialToken.*Specular`), clipped to `shape`.
  func specular(_ strength: Double, in shape: some Shape = Rectangle()) -> some View {
    modifier(Specular(strength: strength, shape: shape))
  }
}

private struct Specular<S: Shape>: ViewModifier {
  let strength: Double
  let shape: S
  @EffectiveAppearance private var appearance

  func body(content: Content) -> some View {
    content.overlay {
      if appearance.material != .solid, strength > 0 {
        shape.fill(sweep).allowsHitTesting(false)
      }
    }
  }

  /// The mockup's 118° gradient: a bright corner, a clear middle and a thin streak.
  private var sweep: LinearGradient {
    let white = Color.white
    return LinearGradient(
      stops: [
        .init(color: white.opacity(strength), location: 0),
        .init(color: white.opacity(strength * 0.25), location: 0.18),
        .init(color: white.opacity(0), location: 0.30),
        .init(color: white.opacity(0), location: 0.64),
        .init(color: white.opacity(strength * 0.4), location: 0.66),
        .init(color: white.opacity(0), location: 0.72),
      ],
      startPoint: .topLeading,
      endPoint: .bottomTrailing
    )
  }
}
