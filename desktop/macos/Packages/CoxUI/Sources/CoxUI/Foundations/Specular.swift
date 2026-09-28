// `.specular(_:in:)` (DS§3.5, DS§6.1): the diagonal sweep and streak that make glass read as
// glass, the mockup's `.window:after`. Separate so the sweep's shape lives in one place; it
// draws nothing when the effective appearance has no specular — Solid, so Reduce Transparency
// removes it too, and Increase Contrast (A89).

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
      if appearance.specular > 0, strength > 0 {
        shape.fill(sweep).allowsHitTesting(false)
      }
    }
  }

  private var sweep: LinearGradient {
    LinearGradient(
      stops: Appearance.sweep(strength).map {
        .init(color: Color.white.opacity($0.opacity), location: $0.location)
      },
      startPoint: .topLeading,
      endPoint: .bottomTrailing
    )
  }
}

extension Appearance {
  /// The mockup's 118° gradient at `strength`: a bright corner, a clear middle and a thin
  /// streak, as white opacities by location. Shared with the transcript's AppKit bubble
  /// (`sweepStops`), so both draw one sweep.
  static func sweep(_ strength: Double) -> [(opacity: Double, location: Double)] {
    [
      (strength, 0), (strength * 0.25, 0.18), (0, 0.30), (0, 0.64), (strength * 0.4, 0.66),
      (0, 0.72),
    ]
  }
}
