// The inspector's Context tab (T37.29.3.1, DT§5.1 Context & Cost): the window's split and the
// turn's cache hit from the token meter's latest `UsageView`, and "Compact now". Here, not in
// CoxUI, because the meter's figures decide what the tab shows (DS§1); the app copies them into
// `ContextTab.State` field for field. `CostHistoryState` is the tab's cost by turn, read from
// the ledger when the tab asks (T37.29.3.2), with the project's spend as its footnote
// (T37.29.3.3). The budget comes later.

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

/// The tab's "Cost by turn": cox-app's `TurnCosts` as `KeyValueGrid` rows, the session total
/// last. Empty, so the section hides, before the ledger has a row.
public struct CostHistoryState: Equatable, Sendable {
  /// `KeyValueGrid.Row`.
  public struct Row: Equatable, Sendable {
    public var label: String
    public var values: [String]
    public var isDetail = false
  }

  /// `In`, `Out`, `Cache r/w`, `$`.
  public var columns: [String] = []
  /// A row per turn, its subagents as detail rows, then `Session`.
  public var rows: [Row] = []
  /// `Project cox today: $3.18 · this week: $21.40. …`, shown even before this session spent.
  public var footnote = ""

  public init() {}

  public init(_ costs: TurnCosts) {
    footnote = costs.project
    guard !costs.rows.isEmpty else { return }
    let row = { (cost: CostRow) in
      Row(label: cost.label, values: cost.values, isDetail: cost.detail)
    }
    (columns, rows) = (costs.columns, costs.rows.map(row) + [row(costs.total)])
  }
}

extension SessionStore {
  /// The cost by turn, read from the core when the tab asks (T37.29.3.2).
  public func costHistory() async throws -> CostHistoryState {
    CostHistoryState(try await session.turnCosts())
  }

  /// The Context tab over the meter's latest figures; it follows every `usage` patch.
  public var contextTab: ContextTabState { ContextTabState(usage) }

  /// The tab's "Compact now": the same manual compaction as `/compact` with no focus.
  public func compactNow() async throws {
    _ = try await send(.compact(focus: nil))
  }
}
