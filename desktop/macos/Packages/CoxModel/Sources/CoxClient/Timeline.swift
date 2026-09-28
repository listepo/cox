// The timeline as Swift values (DT§4.3): `Block`, `TimelinePatch` and the
// records they carry, field for field as cox-ffi exports them. Separate from
// the generated bindings so the stores, views and tests hold them without
// linking the Rust library; `LiveCoreClient` (CoxCore) converts the
// generated values, and `Decodable` reads the serde JSON a recorded fixture
// holds (DT§8). No key strategy: `convertFromSnakeCase` would also rename
// the keys inside an edited tool input.

import Foundation

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
        by: DecidedBy?)
    case question(call: String, question: String, options: [String], answer: String?)
    case task(task: String, label: String, tier: Tier, done: Bool, costUsd: Double, exitCode: Int32?)
    case compaction(beforeTokens: UInt32, afterTokens: UInt32, reason: CompactReason, summary: String?)
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
    case readOnly = "read_only", write, exec, destructive
}

public enum Tier: String, Equatable, Sendable, Decodable { case cheap, code, think }

public enum Level: String, Equatable, Sendable, Decodable { case info, warn, budget, security }

public enum DecidedBy: String, Equatable, Sendable, Decodable { case user, rule, session, policy, hook }

public enum ApprovalPolicy: String, Equatable, Sendable, Decodable {
    case untrusted, onRequest = "on-request", onFailure = "on-failure", never
}

public enum CompactReason: String, Equatable, Sendable, Decodable {
    case preCall = "pre-call", postTurn = "post-turn", manual, contextTooLong = "context-too-long"
}

public enum StyleToken: String, Equatable, Sendable, Decodable {
    case text, dim, accent, user, agent, tool, ok, warn, error
    case diffAdd = "diff_add", diffDel = "diff_del", diffHunk = "diff_hunk", border, selection
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
        case inputTokens = "input_tokens", outputTokens = "output_tokens"
        case cacheReadTokens = "cache_read_tokens", cacheWriteTokens = "cache_write_tokens"
        case estimated, costUsd = "cost_usd", latencyMs = "latency_ms"
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

    enum CodingKeys: String, CodingKey { case session, turn, contextTokens = "context_tokens" }
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
        (self.sent, self.received, self.cacheRead, self.cacheWrite) = (sent, received, cacheRead, cacheWrite)
        (self.uncached, self.costUsd, self.calls, self.estimated) = (uncached, costUsd, calls, estimated)
    }

    enum CodingKeys: String, CodingKey {
        case sent, received, cacheRead = "cache_read", cacheWrite = "cache_write", uncached
        case costUsd = "cost_usd", calls, estimated
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
        (self.turn, self.tally, self.thinkingTokens, self.ttftMs) = (turn, tally, thinkingTokens, ttftMs)
        (self.tokPerS, self.exact, self.sparkline, self.done) = (tokPerS, exact, sparkline, done)
    }

    enum CodingKeys: String, CodingKey {
        case turn, tally, thinkingTokens = "thinking_tokens", ttftMs = "ttft_ms"
        case tokPerS = "tok_per_s", exact, sparkline, done
    }
}

// MARK: - serde's tagged enums

/// Any key, so one container reads every variant of an internally tagged
/// enum (`#[serde(tag = "type")]`).
struct AnyKey: CodingKey {
    let stringValue: String
    var intValue: Int? { nil }
    init(_ name: String) { stringValue = name }
    init(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

extension KeyedDecodingContainer where K == AnyKey {
    func callAsFunction<T: Decodable>(_ key: String) throws -> T {
        try decode(T.self, forKey: AnyKey(key))
    }

    func optional<T: Decodable>(_ key: String) throws -> T? {
        try decodeIfPresent(T.self, forKey: AnyKey(key))
    }

    func unknown(_ tag: String) -> DecodingError {
        .dataCorrupted(.init(codingPath: codingPath, debugDescription: "unknown variant \(tag)"))
    }
}

extension Decoder {
    func fields() throws -> KeyedDecodingContainer<AnyKey> { try container(keyedBy: AnyKey.self) }
}

extension BlockKind: Decodable {
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        let tag: String = try f("type")
        switch tag {
        case "user": self = try .user(text: f("text"), attachments: f("attachments"))
        case "assistant": self = try .assistant(text: f("text"), doc: f("doc"))
        case "thinking": self = try .thinking(text: f("text"))
        case "tool":
            self = try .tool(
                tool: f("tool"), summary: f("summary"), icon: f("icon"), risk: f("risk"),
                state: f("state"), tail: f("tail"), archive: f.optional("archive"),
                diff: f.optional("diff"), durationMs: f("duration_ms"))
        case "tool_group":
            self = try .toolGroup(summary: f("summary"), children: f("children"), state: f("state"))
        case "approval":
            self = try .approval(
                call: f("call"), tool: f("tool"), summary: f("summary"), why: f("why"),
                source: f.optional("source"), decision: f.optional("decision"), by: f.optional("by"))
        case "question":
            self = try .question(
                call: f("call"), question: f("question"), options: f("options"),
                answer: f.optional("answer"))
        case "task":
            self = try .task(
                task: f("task"), label: f("label"), tier: f("tier"), done: f("done"),
                costUsd: f("cost_usd"), exitCode: f.optional("exit_code"))
        case "compaction":
            self = try .compaction(
                beforeTokens: f("before_tokens"), afterTokens: f("after_tokens"), reason: f("reason"),
                summary: f.optional("summary"))
        case "checkpoint": self = try .checkpoint(files: f("files"))
        case "notice": self = try .notice(level: f("level"), text: f("text"))
        case "error": self = try .error(text: f("text"), fatal: f("fatal"))
        case "turn_meta":
            self = try .turnMeta(
                model: f("model"), tier: f("tier"), usage: f.optional("usage"), stop: f.optional("stop"))
        default: throw f.unknown(tag)
        }
    }
}

extension TimelinePatch: Decodable {
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        let tag: String = try f("op")
        switch tag {
        case "reset": self = try .reset(blocks: f("blocks"))
        case "upsert": self = try .upsert(block: f("block"), after: f.optional("after"))
        case "append_text": self = try .appendText(id: f("id"), text: f("text"))
        case "doc_tail": self = try .docTail(id: f("id"), from: f("from"), blocks: f("blocks"))
        case "remove": self = try .remove(id: f("id"))
        case "usage": self = try .usage(usage: f("usage"))
        default: throw f.unknown(tag)
        }
    }
}

extension StopReason: Decodable {
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        let tag: String = try f("type")
        switch tag {
        case "end_turn": self = .endTurn
        case "max_turns": self = .maxTurns
        case "interrupted": self = .interrupted
        case "budget": self = .budget
        case "error": self = .error
        case "refusal": self = try .refusal(detail: f("detail"))
        default: throw f.unknown(tag)
        }
    }
}

extension Decision: Decodable {
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        let tag: String = try f("type")
        switch tag {
        case "allow": self = .allow
        case "allow_for_session": self = .allowForSession
        case "deny": self = try .deny(reason: f("reason"))
        case "edit":
            let input: JSONValue = try f("input")
            self = .edit(input: try input.text())
        default: throw f.unknown(tag)
        }
    }
}

extension Why: Decodable {
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        let tag: String = try f("type")
        switch tag {
        case "rule_ask": self = try .ruleAsk(rule: f("rule"))
        case "risk": self = try .risk(risk: f("risk"))
        case "sandbox_denied": self = try .sandboxDenied(detail: f("detail"))
        case "policy": self = try .policy(policy: f("policy"))
        default: throw f.unknown(tag)
        }
    }
}

extension DocBlock: Decodable {
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        let tag: String = try f("type")
        switch tag {
        case "text": self = try .text(kind: f("kind"), lines: f("lines"))
        case "code": self = try .code(lang: f("lang"), lines: f("lines"))
        case "table": self = try .table(rows: f("rows"))
        case "rule": self = .rule
        default: throw f.unknown(tag)
        }
    }
}

extension TextKind: Decodable {
    /// Externally tagged: `"paragraph"`, or `{"heading": 2}`.
    public init(from decoder: any Decoder) throws {
        if let unit = try? decoder.singleValueContainer().decode(String.self) {
            switch unit {
            case "paragraph": self = .paragraph
            case "list": self = .list
            case "quote": self = .quote
            default: throw try decoder.fields().unknown(unit)
            }
            return
        }
        self = try .heading(decoder.fields()("heading"))
    }
}

extension Span: Decodable {
    /// serde omits every field left at its default; `rgb` is `[r, g, b]`.
    public init(from decoder: any Decoder) throws {
        let f = try decoder.fields()
        text = try f.optional("text") ?? ""
        token = try f.optional("token") ?? .text
        rgb = try (f.optional("rgb") as [UInt8]?).map { $0.reduce(0) { $0 << 8 | UInt32($1) } }
        bold = try f.optional("bold") ?? false
        italic = try f.optional("italic") ?? false
        strike = try f.optional("strike") ?? false
        underline = try f.optional("underline") ?? false
        link = try f.optional("link")
    }
}

/// Any JSON, read only to turn an edited tool input back into JSON text.
enum JSONValue: Codable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let b = try? c.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else if let s = try? c.decode(String.self) {
            self = .string(s)
        } else if let a = try? c.decode([JSONValue].self) {
            self = .array(a)
        } else {
            self = .object(try c.decode([String: JSONValue].self))
        }
    }

    func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let b): try c.encode(b)
        case .number(let n): try c.encode(n)
        case .string(let s): try c.encode(s)
        case .array(let a): try c.encode(a)
        case .object(let o): try c.encode(o)
        }
    }

    func text() throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        return String(decoding: try encoder.encode(self), as: UTF8.self)
    }
}
