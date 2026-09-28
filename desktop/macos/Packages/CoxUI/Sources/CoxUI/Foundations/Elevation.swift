// `.elevation(_:)` (DS§3.4, DS§6.1): the only place a shadow is drawn, so the Depth setting
// scales every lifted thing at once. Drop shadows sit behind the view; inset layers are the
// top-edge highlight drawn inside the view's shape.

import SwiftUI

extension View {
  /// Lifts the view to `level`, its highlight following a rounded shape of `cornerRadius`.
  func elevation(_ level: ElevationToken, cornerRadius: CGFloat = 0) -> some View {
    modifier(
      Elevation(
        level: level,
        shape: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)))
  }
}

private struct Elevation: ViewModifier {
  let level: ElevationToken
  let shape: RoundedRectangle
  @EffectiveAppearance private var appearance

  /// Depth scales every level but the window's (DS§3.4).
  private var scale: Double { level == .e5 ? 1 : appearance.depth }

  func body(content: Content) -> some View {
    content
      .modifier(DropShadows(layers: level.layers.filter { !$0.inset }, scale: scale))
      .overlay { highlights }
  }

  private var highlights: some View {
    ZStack {
      ForEach(Array(level.layers.filter(\.inset).enumerated()), id: \.offset) { _, layer in
        shape.subtracting(
          shape.inset(by: layer.spread).offset(x: layer.x, y: layer.y)
        )
        .fill(layer.color.opacity(scale))
      }
    }
    .allowsHitTesting(false)
  }
}

/// Stacks one `.shadow` per layer; CSS blur is twice SwiftUI's radius.
private struct DropShadows: ViewModifier {
  let layers: [ShadowLayer]
  let scale: Double

  func body(content: Content) -> some View {
    layers.reduce(AnyView(content)) { view, layer in
      AnyView(
        view.shadow(
          color: layer.color.opacity(scale), radius: layer.blur / 2, x: layer.x,
          y: layer.y * scale))
    }
  }
}
