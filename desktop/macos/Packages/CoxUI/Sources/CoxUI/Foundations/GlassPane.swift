// `.glassPane(_:)` (DS§3.5, DS§6.1): the one place a pane chooses glass or a solid surface, so
// the material setting, the readable floor and Reduce Transparency apply to every pane alike.

import SwiftUI

extension View {
  /// Backs the view with the pane material in `shape`: `surface` tinted to the appearance's
  /// opacity over glass, or opaque `surface` in Solid.
  func glassPane(
    _ shape: some Shape, surface: Color = Color(.surfaceWindow), role: SurfaceRole = .chrome
  ) -> some View {
    modifier(GlassPane(shape: shape, surface: surface, role: role))
  }
}

private struct GlassPane<S: Shape>: ViewModifier {
  let shape: S
  let surface: Color
  let role: SurfaceRole
  @EffectiveAppearance private var appearance

  func body(content: Content) -> some View {
    let tinted = content.background {
      shape.fill(surface.opacity(appearance.backgroundOpacity(role)))
    }
    switch appearance.material {
    case .solid:
      tinted
    case .frosted:
      tinted.glassEffect(.regular, in: shape).specular(appearance.specular, in: shape)
    case .glossy:
      tinted.glassEffect(.clear, in: shape).specular(appearance.specular, in: shape)
    }
  }
}
