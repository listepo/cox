// `PreviewState` fixtures for the inspector's Context tab (T37.29.3.1): a turn's split of a 200k
// window as `cox_app::MeterText` formats it, and the same split with the window unknown. Separate
// from `PreviewState+Inspector.swift` so the inspector's tabs, built in parallel, add their
// fixtures without editing one file.

import SwiftUI

extension PreviewState {
  /// Mockup screen 10's window: the bar's `turn` mix of 200k, with the parts' tokens.
  static var contextTab: ContextTab.State {
    var state = ContextTab.State(context: "Context · 76.4k", share: "38% of 200k")
    let figures = [("System", "6.2k"), ("Tools", "9.8k"), ("Instructions", "11.5k")]
    state.parts = zip(bars[2].segments, figures + [("History", "48.9k")]).map {
      ContextTab.Part(kind: $0.kind, fraction: $0.fraction, label: $1.0, tokens: $1.1)
    }
    (state.free, state.cacheHit) = ("123.6k", "94% this turn")
    return state
  }

  /// A model whose window the catalog does not know: the bar is the whole context, nothing free.
  static var contextNoWindow: ContextTab.State {
    var state = contextTab
    let whole = state.parts.reduce(0) { $0 + $1.fraction }
    for index in state.parts.indices { state.parts[index].fraction /= whole }
    (state.share, state.free) = ("", "")
    return state
  }
}

/// The inspector on its Context tab, as tall as the smallest window.
struct ContextInspectorSample: View {
  let state: ContextTab.State

  var body: some View {
    Inspector(selection: .context, content: ContextTab(state: state) { _ in }) { _ in }
      .frame(height: Size.windowMinHeight)
  }
}
