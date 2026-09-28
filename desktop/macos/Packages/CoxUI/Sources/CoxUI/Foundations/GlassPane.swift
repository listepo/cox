// `.glassPane(_:)` (DS§3.5, DS§6.1): the one place a pane chooses glass or a solid surface, so
// the material setting, the readable floor and Reduce Transparency apply to every pane alike.

import SwiftUI

extension View {
  /// Backs the view with the pane material in `shape`: `surface` tinted to the appearance's
  /// opacity over glass, or opaque `surface` in Solid. `frosts: false` is for the window's own
  /// panes, which sit on the behind-window blur: what is under them is frosted already, and a
  /// second glass layer would frost it again and read as white, so they only tint.
  func glassPane(
    _ shape: some Shape, surface: Color = Color(.surfaceWindow), role: SurfaceRole = .chrome,
    frosts: Bool = true
  ) -> some View {
    modifier(GlassPane(shape: shape, surface: surface, role: role, frosts: frosts))
  }
}

private struct GlassPane<S: Shape>: ViewModifier {
  let shape: S
  let surface: Color
  let role: SurfaceRole
  let frosts: Bool
  @EffectiveAppearance private var appearance

  func body(content: Content) -> some View {
    let tinted = content.background {
      shape.fill(surface.opacity(appearance.backgroundOpacity(role)))
    }
    switch appearance.material {
    case .solid:
      tinted
    case _ where !frosts:
      tinted.specular(appearance.specular, in: shape)
    case .frosted:
      tinted.glassEffect(.regular, in: shape).specular(appearance.specular, in: shape)
    case .glossy:
      tinted.glassEffect(.clear, in: shape).specular(appearance.specular, in: shape)
    }
  }
}
