// The inspector's Context tab (T37.29.3.1, DT§5.1 Context & Cost): the window's split and the
// turn's cache hit from the token meter's latest `UsageView`, and "Compact now". Here, not in
// CoxUI, because the meter's figures decide what the tab shows (DS§1); the app copies them into
// `ContextTab.State` field for field. Per-turn cost, totals and the budget come later.

import CoxClient

public struct ContextTabState: Equatable, Sendable {
  public var split = ContextSplit()
  /// `94% this turn`; empty before a turn sent anything.
  public var cacheHit = ""

  public init() {}

  /// Empty until the first `usage` patch.
  public init(_ usage: UsageView?) {
    guard let text = usage?.text else { return }
    (split, cacheHit) = (ContextSplit(text), text.cacheHit)
  }
}

extension SessionStore {
  /// The Context tab over the meter's latest figures; it follows every `usage` patch.
  public var contextTab: ContextTabState { ContextTabState(usage) }

  /// The tab's "Compact now": the same manual compaction as `/compact` with no focus.
  public func compactNow() async throws {
    _ = try await send(.compact(focus: nil))
  }
}
