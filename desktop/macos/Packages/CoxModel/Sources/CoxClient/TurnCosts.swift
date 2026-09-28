// The Context tab's cost history (T37.29.3.2, DT§5.1), field for field as cox-ffi exports
// `cox_app::TurnCosts`: the ledger's rows by turn, subagents under their turn, and the session
// total, every figure formatted by the core. Separate from the timeline because it answers a
// call, not a patch.

public struct TurnCosts: Equatable, Sendable {
  /// The value columns' headers: `In`, `Out`, `Cache r/w`, `$`.
  public var columns: [String]
  /// A row per turn in order, each turn's subagents under it as detail rows.
  public var rows: [CostRow]
  /// `Session`: every row summed.
  public var total: CostRow

  public init(columns: [String] = [], rows: [CostRow] = [], total: CostRow = CostRow()) {
    (self.columns, self.rows, self.total) = (columns, rows, total)
  }
}

/// One line of the grid: `1 · code`, then a value per column.
public struct CostRow: Equatable, Sendable {
  public var label: String
  public var values: [String]
  /// A subagent's row, drawn indented under its turn.
  public var detail: Bool

  public init(label: String = "", values: [String] = [], detail: Bool = false) {
    (self.label, self.values, self.detail) = (label, values, detail)
  }
}
