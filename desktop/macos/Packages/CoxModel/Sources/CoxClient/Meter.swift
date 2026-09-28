// The token meter's text (T37.25, A98): `MeterText`, its grid rows and the context split, the
// figures `cox_app::MeterText` formats for the meter and its popover, field for field as cox-ffi
// exports them. Separate from `Timeline.swift` so the meter's values grow without that file.

/// `cox_app::MeterText`: every figure of the token meter and popover as it is shown.
public struct MeterText: Equatable, Sendable, Decodable {
  public var sent = "", received = "", rate = "", spoken = ""
  public var heading = "", phase = "", rateUnit = "", rateDetail = ""
  public var rows: [MeterRow] = []
  public var context = "", footnote = ""
  /// `0.4% of 1M`, the context's share of the model's window; empty while the window is unknown.
  public var contextShare = ""
  /// System, tools, instructions and history, in that order; empty until the first request.
  public var contextParts: [ContextPart] = []

  public init() {}

  enum CodingKeys: String, CodingKey {
    case sent, received, rate, spoken, heading, phase, rows, context, footnote
    case rateUnit = "rate_unit"
    case rateDetail = "rate_detail"
    case contextShare = "context_share"
    case contextParts = "context_parts"
  }
}

/// `cox_app::ContextPart`: one part of the context bar and its legend (A98).
public struct ContextPart: Equatable, Sendable, Decodable {
  /// `system`, `tools`, `instructions` or `history`: the part's colour role, `context.<kind>`.
  public var kind: String
  /// `System` and `3.5k`.
  public var label, tokens: String
  /// The part's width in the bar, a fraction of the window.
  public var share: Double

  public init(kind: String, label: String, tokens: String, share: Double) {
    (self.kind, self.label, self.tokens, self.share) = (kind, label, tokens, share)
  }
}

/// One line of the token popover's grid: a label and its turn and session figures.
public struct MeterRow: Equatable, Sendable, Decodable {
  public var label, turn, session: String
  public var detail: Bool

  public init(label: String, turn: String, session: String, detail: Bool) {
    (self.label, self.turn, self.session, self.detail) = (label, turn, session, detail)
  }
}
