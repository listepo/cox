// `ContextTab` (DS§6.4 row `ContextTab`, mockup screen 10's context block; DT§5.1 Context &
// Cost): the inspector's third tab — how the model's window is split between system, tools,
// instruction files and history, what is left, the turn's cache hit, and "Compact now". Separate
// so the `Inspector` frame stays a slot and each tab is its own view, fed plain values the app
// copies from the core's token meter (T37.29.3.1). Per-turn cost and the budget come later.

import SwiftUI

/// An `InspectorSection` headed by the context and its share of the window: the StackedBar, a
/// legend row per part and one for what is free, and "Compact now"; then the cache hit. Before
/// the core sent the split, one quiet line.
struct ContextTab: View {
  /// What the tab shows, formatted by the core.
  struct State: Equatable, Sendable {
    /// `Context · 76.4k` and `7.6% of 1M` (empty while the window is unknown).
    var context = "", share = ""
    /// In the bar's order; none hides the section.
    var parts: [Part] = []
    /// `923.6k`, the window less the parts; empty while the window is unknown.
    var free = ""
    /// `94% this turn`.
    var cacheHit = ""
  }

  /// A part of the window: its colour role and share of the bar, `System` and `7.6k`.
  struct Part: Equatable, Sendable {
    var kind: StackedBar.Kind
    var fraction: Double
    var label, tokens: String
  }

  /// What the tab asks the app to do.
  enum Intent: Equatable, Sendable {
    /// Compact the conversation now, as `/compact` does.
    case compact
  }

  let state: State
  let send: @MainActor (Intent) -> Void

  /// The mockup's legend swatch, 8 pt, as the token popover draws it.
  private static let swatch: CGFloat = 8

  var body: some View {
    VStack(alignment: .leading, spacing: Space.xl) {
      if !state.parts.isEmpty {
        InspectorSection(state.context) {
          figure(state.share)
        } rows: {
          VStack(alignment: .leading, spacing: Space.l) {
            StackedBar(state.parts.map { .init(kind: $0.kind, fraction: $0.fraction) })
            legend
            Button("Compact now") { send(.compact) }
              .buttonStyle(CoxButtonStyle(.secondary, size: .small))
          }
        }
      }
      if !state.cacheHit.isEmpty {
        SectionHeader("Cache hit") { figure(state.cacheHit) }
      }
      if state.parts.isEmpty && state.cacheHit.isEmpty {
        Text("No context yet")
          .textStyle(.caption)
          .foregroundStyle(Color(.textSecondary))
      }
    }
  }

  /// A swatch, the part and its tokens per row; the free share last and quieter.
  private var legend: some View {
    Grid(alignment: .leading, horizontalSpacing: Space.s, verticalSpacing: Space.xs) {
      ForEach(state.parts, id: \.kind) { part in
        row(part.kind.colour, part.label, part.tokens, text: Color(.textPrimary))
      }
      if !state.free.isEmpty {
        row(Color(.fillSecondary), "Free", state.free, text: Color(.textSecondary))
      }
    }
    .textStyle(.footnote, tabularDigits: true)
  }

  private func row(_ swatch: Color, _ label: String, _ tokens: String, text: Color) -> some View {
    GridRow {
      RoundedRectangle(cornerRadius: Radius.xs).fill(swatch)
        .frame(width: Self.swatch, height: Self.swatch)
      Text(label).foregroundStyle(text).frame(maxWidth: .infinity, alignment: .leading)
      Text(tokens).foregroundStyle(Color(.textSecondary)).gridColumnAlignment(.trailing)
    }
  }

  /// A header's trailing figure.
  private func figure(_ text: String) -> some View {
    Text(text)
      .textStyle(.label, tabularDigits: true)
      .foregroundStyle(Color(.textSecondary))
  }
}

#Preview("context") { PreviewMatrix { ContextInspectorSample(state: PreviewState.contextTab) } }
#Preview("no window") {
  PreviewMatrix { ContextInspectorSample(state: PreviewState.contextNoWindow) }
}
#Preview("empty") { PreviewMatrix { ContextInspectorSample(state: .init()) } }
