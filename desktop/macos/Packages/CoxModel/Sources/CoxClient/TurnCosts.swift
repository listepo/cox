// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

// The Context tab's cost history (T37.29.3.2, DT§5.1), field for field as cox-ffi exports
// `cox_app::TurnCosts`: the ledger's rows by turn, subagents under their turn, the session
// total and the project's spend today and this week, every figure formatted by the core.
// Separate from the timeline because it answers a call, not a patch.

public struct TurnCosts: Equatable, Sendable {
  /// The value columns' headers: `In`, `Out`, `Cache r/w`, `$`.
  public var columns: [String]
  /// A row per turn in order, each turn's subagents under it as detail rows.
  public var rows: [CostRow]
  /// `Session`: every row summed.
  public var total: CostRow
  /// The footnote: `Project cox today: $3.18 · this week: $21.40. …` (T37.29.3.3).
  public var project: String

  public init(
    columns: [String] = [], rows: [CostRow] = [], total: CostRow = CostRow(), project: String = ""
  ) {
    (self.columns, self.rows, self.total, self.project) = (columns, rows, total, project)
  }
}

/// One line of the grid: `1 · code` (a subagent's `explore`), then a value per column.
public struct CostRow: Equatable, Sendable {
  public var label: String
  public var values: [String]
  /// A subagent's row, drawn indented under its turn.
  public var detail: Bool

  public init(label: String = "", values: [String] = [], detail: Bool = false) {
    (self.label, self.values, self.detail) = (label, values, detail)
  }
}
