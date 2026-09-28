// One open session as observable state (DT§4.6): the timeline keyed by
// block id in insertion order (`OrderedDictionary`, research.md 9.5.6), the
// token meter and the composer draft. It applies patches and sends intents
// and nothing else; every decision already came from Rust. The rules mirror
// `cox_app::coalesce::apply`, the reference consumer the Rust tests prove.

import CoxClient
import Observation
import OrderedCollections

@Observable
@MainActor
public final class SessionStore {
  public private(set) var blocks: OrderedDictionary<BlockID, Block>
  /// The token meter (DS§7); `nil` until the first `usage` patch.
  public private(set) var usage: UsageView?
  public var draft = ""
  @ObservationIgnored public let session: any SessionClient

  public init(session: any SessionClient) {
    self.session = session
    blocks = Self.keyed(session.snapshot())
  }

  /// Pulls batches until the session closes or the task is cancelled,
  /// applying each in one main-actor hop (DT§4.5).
  public func run() async {
    while !Task.isCancelled, let batch = await session.nextPatches() {
      apply(batch)
    }
  }

  public func send(_ intent: Intent) async throws -> (any SessionClient)? {
    try await session.send(intent)
  }

  /// Told each batch once the store has applied it, so a view that keeps its own copy of the
  /// timeline — the transcript text (T37.23) — splices the same patches.
  @ObservationIgnored public var didApply: (@MainActor ([TimelinePatch]) -> Void)?

  public func apply(_ patches: [TimelinePatch]) {
    for patch in patches { apply(patch) }
    didApply?(patches)
  }

  private func apply(_ patch: TimelinePatch) {
    switch patch {
    case .reset(let all):
      blocks = Self.keyed(all)
    case .upsert(let block, let after):
      if blocks[block.id] != nil {
        blocks[block.id] = block
        return
      }
      // An unknown `after` appends, as in the Rust consumer.
      let index = after.map { blocks.index(forKey: $0).map { $0 + 1 } ?? blocks.count } ?? 0
      blocks.updateValue(block, forKey: block.id, insertingAt: index)
    case .appendText(let id, let text):
      blocks[id]?.kind.append(text)
    case .docTail(let id, let from, let tail):
      blocks[id]?.kind.replaceDoc(from: Int(from), with: tail)
    case .remove(let id):
      blocks.removeValue(forKey: id)
    case .usage(let view):
      usage = view
    }
  }

  private static func keyed(_ all: [Block]) -> OrderedDictionary<BlockID, Block> {
    OrderedDictionary(all.map { ($0.id, $0) }, uniquingKeysWith: { _, later in later })
  }
}

/// Lines a running tool block keeps (`cox_app::patch::TAIL_LINES`).
let tailLines = 5

extension BlockKind {
  /// A patch that does not fit the block's kind changes nothing.
  mutating func append(_ more: String) {
    switch self {
    case .thinking(let text):
      self = .thinking(text: text + more)
    case .tool(
      let tool, let summary, let icon, let risk, let state, let tail, let archive, let diff,
      let durationMs):
      self = .tool(
        tool: tool, summary: summary, icon: icon, risk: risk, state: state,
        tail: lastLines(tail + more), archive: archive, diff: diff, durationMs: durationMs)
    default:
      break
    }
  }

  mutating func replaceDoc(from: Int, with tail: [DocBlock]) {
    guard case .assistant(let text, var doc) = self, from <= doc.blocks.count else { return }
    doc.blocks.replaceSubrange(from..., with: tail)
    self = .assistant(text: text, doc: doc)
  }
}

/// The last `tailLines` lines of `text`, a trailing newline kept
/// (`cox_app::patch::tail`). Bytes, not `Character`s: `\r\n` is one
/// grapheme in Swift but ends a line in Rust.
func lastLines(_ text: String) -> String {
  let bytes = text.utf8
  let newline = UInt8(ascii: "\n")
  var index = bytes.last == newline ? bytes.index(before: bytes.endIndex) : bytes.endIndex
  var seen = 0
  while index > bytes.startIndex {
    index = bytes.index(before: index)
    if bytes[index] == newline {
      seen += 1
      if seen == tailLines { return String(text[bytes.index(after: index)...]) }
    }
  }
  return text
}
