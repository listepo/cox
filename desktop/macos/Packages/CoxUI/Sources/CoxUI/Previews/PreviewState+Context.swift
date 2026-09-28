// `PreviewState` fixtures for the inspector's Context tab (T37.29.3.1): a turn's split of a 200k
// window as `cox_app::MeterText` formats it, the same split with the window unknown, and mockup
// 10's cost by turn as `cox_app::TurnCosts` formats it (T37.29.3.2). Separate
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

  /// Mockup screen 10's cost by turn: two turns, a subagent under the first, the session total.
  static var contextCosts: ContextTab.State {
    var state = contextTab
    state.costColumns = ["In", "Out", "Cache r/w", "$"]
    state.costs = [
      .init(label: "1 · code", values: ["31.4k", "2.2k", "28.0k/3.1k", "0.29"]),
      .init(label: "explore", values: ["9.8k", "600", "0/9.8k", "0.03"], isDetail: true),
      .init(label: "2 · code", values: ["16.8k", "900", "15.9k/800", "0.10"]),
      .init(label: "Session", values: ["58.0k", "3.7k", "43.9k/13.7k", "0.42"]),
    ]
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
