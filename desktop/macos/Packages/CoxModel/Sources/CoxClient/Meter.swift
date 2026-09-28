// The token meter's figures as `cox_app::MeterText` formats them (T37.25, A98): the popover's rows,
// the context split and the cache hit, field for field. Separate from `Timeline.swift`, which
// carries them inside `UsageView`, so the meter's text can grow without that file doing so.

/// `cox_app::MeterText`: every figure of the token meter and popover as it is shown.
public struct MeterText: Equatable, Sendable, Decodable {
  public var sent = "", received = "", rate = "", spoken = ""
  public var heading = "", phase = "", rateUnit = "", rateDetail = ""
  public var rows: [MeterRow] = []
  public var context = "", footnote = ""
  /// `7.6% of 1M` and `923.6k` free; empty while the window is unknown (A98).
  public var contextShare = "", contextFree = ""
  /// System, tools, instructions and history; empty until the first request.
  public var contextParts: [ContextPart] = []
  /// `94% this turn`; empty before a turn sent anything.
  public var cacheHit = ""

  public init() {}

  enum CodingKeys: String, CodingKey {
    case sent, received, rate, spoken, heading, phase, rows, context, footnote
    case rateUnit = "rate_unit"
    case rateDetail = "rate_detail"
    case contextShare = "context_share"
    case contextFree = "context_free"
    case contextParts = "context_parts"
    case cacheHit = "cache_hit"
  }
}

/// `cox_app::ContextPart`: a part of the context window, its tokens and its share of the bar.
public struct ContextPart: Equatable, Sendable, Decodable {
  /// `system`, `tools`, `instructions` or `history`.
  public var kind, label, tokens: String
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
