// The timeline as Swift values (DT§4.3): `Block`, `TimelinePatch` and the
// records they carry, field for field as cox-ffi exports them. Separate from
// the generated bindings so the stores, views and tests hold them without
// linking the Rust library; `LiveCoreClient` (CoxCore) converts the
// generated values, and `TimelineDecoding.swift` reads the serde JSON a
// recorded fixture holds (DT§8). Snake-case fields map through explicit
// `CodingKeys`: `convertFromSnakeCase` would also rename the keys inside an
// edited tool input.

public typealias BlockID = String

public struct Block: Identifiable, Equatable, Sendable, Decodable {
  public var id: BlockID
  /// The turn's ordinal; 0 before the first turn.
  public var turn: UInt32
  public var kind: BlockKind

  public init(id: BlockID, turn: UInt32, kind: BlockKind) {
    (self.id, self.turn, self.kind) = (id, turn, kind)
  }
}

public enum BlockKind: Equatable, Sendable {
  case user(text: String, attachments: [String])
  case assistant(text: String, doc: StyledDoc)
  case thinking(text: String)
  case tool(
    tool: String, summary: String, icon: Icon, risk: Risk, state: ToolState, tail: String,
    archive: ArchiveRef?, diff: Diff?, durationMs: UInt64)
  case toolGroup(summary: String, children: [BlockID], state: ToolState)
  case approval(
    call: String, tool: String, summary: String, why: Why, source: Source?, decision: Decision?,
    // cox-ffi's field name.
    // swiftlint:disable:next identifier_name
    by: DecidedBy?)
  case question(call: String, question: String, options: [String], answer: String?)
  case task(task: String, label: String, tier: Tier, done: Bool, costUsd: Double, exitCode: Int32?)
  case compaction(
    beforeTokens: UInt32, afterTokens: UInt32, reason: CompactReason, summary: String?)
  case checkpoint(files: [String])
  case notice(level: Level, text: String)
  case error(text: String, fatal: Bool)
  case turnMeta(model: String, tier: Tier, usage: Usage?, stop: StopReason?)
}

public enum TimelinePatch: Equatable, Sendable {
  case reset(blocks: [Block])
  /// Replaces the block with this id in place, or inserts it after `after`
  /// (`nil`: first).
  case upsert(block: Block, after: BlockID?)
  case appendText(id: BlockID, text: String)
  /// Replaces an assistant doc's blocks from index `from` on.
  case docTail(id: BlockID, from: UInt32, blocks: [DocBlock])
  case remove(id: BlockID)
  case usage(usage: UsageView)
}

public enum ToolState: String, Equatable, Sendable, Decodable { case running, done, failed }

public enum Icon: String, Equatable, Sendable, Decodable {
  case read, edit, shell, search, web, todo, ask, agent, mcp, tool
}

public enum Risk: String, Equatable, Sendable, Decodable {
  case readOnly = "read_only"
  case write, exec, destructive
}

public enum Tier: String, Equatable, Sendable, Decodable { case cheap, code, think }

public enum Level: String, Equatable, Sendable, Decodable { case info, warn, budget, security }

public enum DecidedBy: String, Equatable, Sendable, Decodable {
  case user, rule, session, policy, hook
}

public enum ApprovalPolicy: String, Equatable, Sendable, Decodable {
  case untrusted
  case onRequest = "on-request"
  case onFailure = "on-failure"
  case never
}

public enum CompactReason: String, Equatable, Sendable, Decodable {
  case preCall = "pre-call"
  case postTurn = "post-turn"
  case manual
  case contextTooLong = "context-too-long"
}

public enum StyleToken: String, Equatable, Sendable, Decodable {
  // cox-ffi's case name.
  // swiftlint:disable:next identifier_name
  case text, dim, accent, user, agent, tool, ok, warn, error
  case diffAdd = "diff_add"
  case diffDel = "diff_del"
  case diffHunk = "diff_hunk"
  case border, selection
}

public struct ArchiveRef: Equatable, Sendable, Decodable {
  public var id: String
  public var bytes: UInt64

  public init(id: String, bytes: UInt64) { (self.id, self.bytes) = (id, bytes) }
}

public struct Diff: Equatable, Sendable, Decodable {
  public var path: String
  public var unified: String

  public init(path: String, unified: String) { (self.path, self.unified) = (path, unified) }
}

public struct Source: Equatable, Sendable, Decodable {
  public var session: String
  public var agent: String?
  public var preset: String?

  public init(session: String, agent: String?, preset: String?) {
    (self.session, self.agent, self.preset) = (session, agent, preset)
  }
}

public struct Usage: Equatable, Sendable, Decodable {
  public var inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens: UInt32
  public var estimated: Bool
  public var costUsd: Double
  public var latencyMs: UInt64

  public init(
    inputTokens: UInt32, outputTokens: UInt32, cacheReadTokens: UInt32, cacheWriteTokens: UInt32,
    estimated: Bool, costUsd: Double, latencyMs: UInt64
  ) {
    (self.inputTokens, self.outputTokens) = (inputTokens, outputTokens)
    (self.cacheReadTokens, self.cacheWriteTokens) = (cacheReadTokens, cacheWriteTokens)
    (self.estimated, self.costUsd, self.latencyMs) = (estimated, costUsd, latencyMs)
  }

  enum CodingKeys: String, CodingKey {
    case inputTokens = "input_tokens"
    case outputTokens = "output_tokens"
    case cacheReadTokens = "cache_read_tokens"
    case cacheWriteTokens = "cache_write_tokens"
    case estimated
    case costUsd = "cost_usd"
    case latencyMs = "latency_ms"
  }
}

public enum StopReason: Equatable, Sendable {
  case endTurn, maxTurns, interrupted, budget, error
  case refusal(detail: String)
}

public enum Decision: Equatable, Sendable {
  case allow, allowForSession
  case deny(reason: String)
  /// `input` is JSON text, as cox-ffi carries it.
  case edit(input: String)
}

extension Decision {
  /// The TUI's deny (`cox-tui` `modal.rs`): the model reads the same words whether the person
  /// denied from the card or from a notification.
  public static let deniedByUser = Decision.deny(reason: "denied by user")
}

public enum Why: Equatable, Sendable {
  case ruleAsk(rule: String)
  case risk(risk: Risk)
  case sandboxDenied(detail: String)
  case policy(policy: ApprovalPolicy)
}

public struct StyledDoc: Equatable, Sendable, Decodable {
  public var blocks: [DocBlock]
  public init(blocks: [DocBlock]) { self.blocks = blocks }
}

public enum DocBlock: Equatable, Sendable {
  case text(kind: TextKind, lines: [[Span]])
  case code(lang: String, lines: [[Span]])
  case table(rows: [[String]])
  case rule
}

public enum TextKind: Equatable, Sendable {
  case paragraph, list, quote
  case heading(UInt8)
}

/// A run of text with one style; `rgb` is a theme colour as `0xRRGGBB`.
public struct Span: Equatable, Sendable {
  public var text: String
  public var token: StyleToken = .text
  public var rgb: UInt32?
  public var bold = false, italic = false, strike = false, underline = false
  public var link: String?

  public init(text: String) { self.text = text }
}

public struct UsageView: Equatable, Sendable, Decodable {
  public var session: Tally
  public var turn: TurnUsage?
  public var contextTokens: UInt32

  public init(session: Tally, turn: TurnUsage?, contextTokens: UInt32) {
    (self.session, self.turn, self.contextTokens) = (session, turn, contextTokens)
  }

  enum CodingKeys: String, CodingKey {
    case session, turn
    case contextTokens = "context_tokens"
  }
}

public struct Tally: Equatable, Sendable, Decodable {
  public var sent, received, cacheRead, cacheWrite, uncached: UInt32
  public var costUsd: Double
  public var calls: UInt32
  public var estimated: Bool

  public init(
    sent: UInt32, received: UInt32, cacheRead: UInt32, cacheWrite: UInt32, uncached: UInt32,
    costUsd: Double, calls: UInt32, estimated: Bool
  ) {
    (self.sent, self.received, self.cacheRead, self.cacheWrite) = (
      sent, received, cacheRead, cacheWrite
    )
    (self.uncached, self.costUsd, self.calls, self.estimated) = (
      uncached, costUsd, calls, estimated
    )
  }

  enum CodingKeys: String, CodingKey {
    case sent, received
    case cacheRead = "cache_read"
    case cacheWrite = "cache_write"
    case uncached
    case costUsd = "cost_usd"
    case calls, estimated
  }
}

public struct TurnUsage: Equatable, Sendable, Decodable {
  public var turn: String
  public var tally: Tally
  public var thinkingTokens: UInt32
  public var ttftMs: UInt64?
  public var tokPerS: Double?
  public var exact: Bool
  public var sparkline: [Double]
  public var done: Bool

  public init(
    turn: String, tally: Tally, thinkingTokens: UInt32, ttftMs: UInt64?, tokPerS: Double?,
    exact: Bool, sparkline: [Double], done: Bool
  ) {
    (self.turn, self.tally, self.thinkingTokens, self.ttftMs) = (
      turn, tally, thinkingTokens, ttftMs
    )
    (self.tokPerS, self.exact, self.sparkline, self.done) = (tokPerS, exact, sparkline, done)
  }

  enum CodingKeys: String, CodingKey {
    case turn, tally
    case thinkingTokens = "thinking_tokens"
    case ttftMs = "ttft_ms"
    case tokPerS = "tok_per_s"
    case exact, sparkline, done
  }
}
