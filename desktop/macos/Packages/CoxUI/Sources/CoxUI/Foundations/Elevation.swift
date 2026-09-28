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

  func body(content: Content) -> some View {
    let layers = level.layers(at: appearance)
    return
      content
      .modifier(DropShadows(layers: layers.filter { !$0.inset }))
      .overlay { highlights(layers.filter(\.inset)) }
  }

  private func highlights(_ layers: [ShadowLayer]) -> some View {
    ZStack {
      ForEach(Array(layers.enumerated()), id: \.offset) { _, layer in
        shape.subtracting(
          shape.inset(by: layer.spread).offset(x: layer.x, y: layer.y)
        )
        .fill(layer.color)
      }
    }
    .allowsHitTesting(false)
  }
}

extension ElevationToken {
  /// The level's layers at `appearance`'s Depth (DS§3.4): each layer's opacity, and a drop
  /// shadow's offset down, scaled by Depth; the window's level ignores it. The highlight also
  /// takes the dark-mode share (A109). Public for the transcript's AppKit bubble (T37.23.9), so
  /// it lifts as `.elevation` does.
  public func layers(at appearance: Appearance) -> [ShadowLayer] {
    let scale = self == .e5 ? 1 : appearance.depth
    let highlight = scale * appearance.highlightStrength(self)
    return layers.map {
      ShadowLayer(
        color: $0.color.opacity($0.inset ? highlight : scale), x: $0.x,
        y: $0.inset ? $0.y : $0.y * scale,
        blur: $0.blur, spread: $0.spread, inset: $0.inset)
    }
  }
}

/// Stacks one `.shadow` per layer; CSS blur is twice SwiftUI's radius.
private struct DropShadows: ViewModifier {
  let layers: [ShadowLayer]

  func body(content: Content) -> some View {
    layers.reduce(AnyView(content)) { view, layer in
      AnyView(view.shadow(color: layer.color, radius: layer.blur / 2, x: layer.x, y: layer.y))
    }
  }
}
