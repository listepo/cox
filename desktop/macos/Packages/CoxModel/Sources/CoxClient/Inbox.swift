// The "Needs you" inbox as Swift values (DT§4.3 Inbox): `InboxItem` and `Need`, cox-ffi's
// records cut to what the app shows, and the one place an item becomes the `HostNote` a
// notification and the Dock badge show. Here, not in CoxCore, so the fixture client hands its
// recorded items to a `PlatformHost` exactly as `HostBridge` hands the live ones (T37.27), and
// the `InboxClient` the sidebar's "Needs you" store reads it through (T37.27.7).

import Synchronization

/// One inbox row; `cox_app::InboxItem`.
public struct InboxItem: Equatable, Sendable, Decodable {
  /// The session the answer goes to.
  public var session: String
  /// The agent that asked.
  public var source: Source?
  public var need: Need
  /// The session closed or moved to another process: shown, not answerable here.
  public var expired: Bool
  /// Arrival order across all sessions.
  public var seq: UInt64

  public init(session: String, source: Source?, need: Need, expired: Bool, seq: UInt64) {
    (self.session, self.source, self.need, self.expired, self.seq) = (
      session, source, need, expired, seq
    )
  }
}

/// What an inbox item asks of the person; `cox_app::Need`.
public enum Need: Equatable, Sendable {
  /// `subject` is what the call acts on: the path, the command.
  case approval(call: String, tool: String, subject: String, why: Why)
  case question(call: String, question: String, options: [String])
  /// A turn stopped on an error or a refusal.
  case failed(text: String)
  case taskDone(task: String, label: String, succeeded: Bool)

  /// The approval or question an answer names; `nil` for news.
  public var call: String? {
    switch self {
    case .approval(let call, _, _, _), .question(let call, _, _): call
    case .failed, .taskDone: nil
    }
  }
}

/// The inbox across every session this process drives, as cox-ffi's `App.inbox`.
public protocol InboxClient: Sendable {
  /// Most urgent first, oldest first within a rank.
  func inbox() -> [InboxItem]
}

extension HostNote {
  /// An inbox item as its notification; `badge` is the count that blocks a turn.
  public init(_ item: InboxItem, badge: Int) {
    func note(_ kind: Kind, _ text: String, call: String? = nil) -> HostNote {
      HostNote(session: item.session, kind: kind, text: text, badge: badge, call: call)
    }
    self =
      switch item.need {
      case .approval(let call, let tool, let subject, _):
        note(.approval, subject.isEmpty ? tool : "\(tool) \(subject)", call: call)
      case .question(let call, let question, _): note(.question, question, call: call)
      case .failed(let text): note(.failed, text)
      case .taskDone(_, let label, let succeeded): note(.taskDone(succeeded: succeeded), label)
      }
  }
}

/// The fixture client's inbox: each recorded note's item from the pull that brings it until
/// its call is answered. Not the core's ranking: items stay in arrival order.
final class FixtureInbox: Sendable {
  private let items = Mutex<[InboxItem]>([])

  var all: [InboxItem] { items.withLock { $0 } }

  func add(_ item: InboxItem) { items.withLock { $0.append(item) } }

  func answer(_ call: String) { items.withLock { $0.removeAll { $0.need.call == call } } }
}

extension Need: Decodable {
  /// serde's internally tagged `Need`; an approval's call is the whole `ToolCall`.
  public init(from decoder: any Decoder) throws {
    let keys = try decoder.fields()
    let tag: String = try keys("type")
    switch tag {
    case "approval":
      let call = try keys.nestedContainer(keyedBy: AnyKey.self, forKey: AnyKey("call"))
      self = try .approval(
        call: call("id"), tool: call("name"), subject: call.optional("subject") ?? "",
        why: keys("why"))
    case "question":
      self = try .question(
        call: keys("call_id"), question: keys("question"), options: keys("options"))
    case "failed": self = try .failed(text: keys("text"))
    case "task_done":
      self = try .taskDone(task: keys("task"), label: keys("label"), succeeded: keys("ok"))
    default: throw keys.unknown(tag)
    }
  }
}
