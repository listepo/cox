// The token meter and popover's values from the core's `UsageView` (T37.25, DS§7): the figures
// `cox_app::MeterText` formatted, copied field by field. Here, beside `SessionComposer`, because
// this package is where CoxUI's values and CoxClient's types meet; nothing here computes a
// figure. The context split (A98, T37.25.2) maps in `TokenPopover.Part.init(_:)`, the one place a
// core `ContextPart` becomes a bar segment and legend, for any view that draws the split.

import CoxClient
import CoxUI

extension TokenMeter.State {
  /// The meter for `usage`; `isRunning` makes the dot glow.
  init(_ usage: UsageView, isRunning: Bool) {
    self.init()
    let text = usage.text
    (sent, received, rate, spoken) = (text.sent, text.received, text.rate, text.spoken)
    isStreaming = isRunning
    sparkline = usage.turn?.sparkline ?? []
  }
}

extension TokenPopover.State {
  /// The popover for `usage`; `isRunning` puts the phase in `accent`.
  init(_ usage: UsageView, isRunning: Bool) {
    self.init()
    let text = usage.text
    (heading, phase, isStreaming) = (text.heading, text.phase, isRunning)
    (rate, rateUnit, rateDetail) = (text.rate, text.rateUnit, text.rateDetail)
    sparkline = usage.turn?.sparkline ?? []
    rows = text.rows.map {
      TokenPopover.Row(label: $0.label, turn: $0.turn, session: $0.session, isDetail: $0.detail)
    }
    (context, contextShare, footnote) = (text.context, text.contextShare, text.footnote)
    parts = text.contextParts.compactMap(TokenPopover.Part.init)
  }
}

extension TokenPopover.Part {
  /// The bar segment and legend (`System 3.5k`) of `part`; `nil` for a kind this build has no
  /// colour role for, so a newer core's part is skipped rather than drawn in the wrong colour.
  init?(_ part: ContextPart) {
    guard let kind = StackedBar.Kind(rawValue: part.kind) else { return nil }
    self.init(kind: kind, fraction: part.share, legend: "\(part.label) \(part.tokens)")
  }
}
