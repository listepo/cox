// The seam between the stores and the Rust core (DT§4.1, §4.6): the
// `CoreClient` protocol CoxModel depends on instead of the FFI, and the
// fixture client that replays a recorded patch stream (DT§8) so every view,
// preview and test runs without Rust. `LiveCoreClient` (CoxCore) is the
// other implementation.

import Foundation
import Synchronization

/// What `CoreClient.open` opens: a new session in `cwd`, or `resume`'s.
/// `theme` is the syntect theme code blocks are highlighted with.
public struct OpenSession: Equatable, Sendable {
  public var cwd: String
  public var resume: String?
  public var theme: String

  public init(cwd: String, resume: String? = nil, theme: String) {
    (self.cwd, self.resume, self.theme) = (cwd, resume, theme)
  }
}

public protocol CoreClient: Sendable {
  func open(_ request: OpenSession) async throws -> any SessionClient
}

/// One open session, as cox-ffi's `SessionHandle`.
public protocol SessionClient: AnyObject, Sendable {
  var id: String { get }
  /// Every block; the next pull continues from here.
  func snapshot() -> [Block]
  /// The next batch, at most one per frame; `nil` once closed.
  func nextPatches() async -> [TimelinePatch]?
  /// Returns at once for a turn; a fork or handoff returns its child.
  func send(_ intent: Intent) async throws -> (any SessionClient)?
  /// Rows for the composer's token, `@que…` or `/que…`, best first, at most `limit`
  /// (`cox_app::Completer`, DT§5.3).
  func complete(_ token: String, limit: UInt32) -> [Completion]
  /// This session's earlier prompts, newest first, at most `limit`: what ↑ walks in an empty
  /// composer (`cox_app::live::LiveSession::history`).
  func history(limit: UInt32) throws -> [String]
  /// What the inspector's Changes tab lists (`cox_app::live::LiveSession::changes`, T37.29.1).
  func changes() async throws -> Changes
  /// Stops the pull; the session keeps running (DT§4.5).
  func close()
}

/// A recorded stream: what `record.rs` (cox-ffi) writes to
/// `desktop/macos/Fixtures/*.json`.
public struct Fixture: Equatable, Sendable, Decodable {
  /// The batches pulled from a session opened fresh, in order.
  public var batches: [[TimelinePatch]]
  /// Each inbox item the host was told about, with the batch it came by.
  public var notes: [Note]
  /// The session's blocks once the last batch was pulled.
  public var snapshot: [Block]

  /// One `AppHost.notify` call of the recording.
  public struct Note: Equatable, Sendable, Decodable {
    public var batch: Int
    public var item: InboxItem
    public var badge: Int

    public init(batch: Int, item: InboxItem, badge: Int) {
      (self.batch, self.item, self.badge) = (batch, item, badge)
    }
  }

  public init(batches: [[TimelinePatch]], notes: [Note] = [], snapshot: [Block]) {
    (self.batches, self.notes, self.snapshot) = (batches, notes, snapshot)
  }

  /// A fixture recorded before notes were (T37.27) has none.
  public init(from decoder: any Decoder) throws {
    let keys = try decoder.fields()
    batches = try keys("batches")
    notes = try keys.optional("notes") ?? []
    snapshot = try keys("snapshot")
  }

  public init(contentsOf url: URL) throws {
    self = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
  }
}

public struct FixtureCoreClient: CoreClient {
  public let fixture: Fixture
  public let completions: [Completion]
  let host: (any PlatformHost)?
  let waitsForYou: Bool
  /// Shared by every session this client opens, as the core's inbox spans sessions.
  let pending = FixtureInbox()

  /// `host` is told each recorded note as its batch is pulled. With `waitsForYou`, a batch
  /// that leaves an approval or question pending holds the next one until it is answered.
  public init(
    fixture: Fixture, completions: [Completion] = [], host: (any PlatformHost)? = nil,
    waitsForYou: Bool = false
  ) {
    (self.fixture, self.completions, self.host, self.waitsForYou) =
      (fixture, completions, host, waitsForYou)
  }

  public func open(_ request: OpenSession) async throws -> any SessionClient {
    FixtureSession(
      fixture: fixture, completions: completions, host: host, waitsForYou: waitsForYou,
      inbox: pending)
  }
}

extension FixtureCoreClient: InboxClient {
  public func inbox() -> [InboxItem] { pending.all }
}

/// Hands out the recorded batches one pull at a time and keeps what was
/// sent, so a test can check the intents a store emitted. Completes from a
/// fixed list instead of the Rust completer, serves a fixed prompt history and lists fixed
/// changes. Waiting on the person, it plays the core: the turn resumes when the card is
/// answered.
public final class FixtureSession: SessionClient {
  public let id = "fixture"
  private let fixture: Fixture
  private let completions: [Completion]
  private let host: (any PlatformHost)?
  private let waitsForYou: Bool
  /// Newest first, as the core returns them.
  private let prompts: [String]
  private let fixedChanges: Changes
  private let inbox: FixtureInbox
  private let state = Mutex(State())

  private struct State {
    var next = 0
    var closed = false
    var sent: [Intent] = []
    /// The calls the last batch left waiting on the person.
    var waiting: Set<String> = []
    /// The pull parked until they are answered.
    var parked: CheckedContinuation<Void, Never>?
  }

  public convenience init(
    fixture: Fixture, completions: [Completion] = [], host: (any PlatformHost)? = nil,
    waitsForYou: Bool = false, prompts: [String] = [], changes: Changes = Changes()
  ) {
    self.init(
      fixture: fixture, completions: completions, host: host, waitsForYou: waitsForYou,
      prompts: prompts, changes: changes, inbox: FixtureInbox())
  }

  init(
    fixture: Fixture, completions: [Completion], host: (any PlatformHost)?, waitsForYou: Bool,
    prompts: [String] = [], changes: Changes = Changes(), inbox: FixtureInbox
  ) {
    (self.fixture, self.completions, self.host, self.waitsForYou) =
      (fixture, completions, host, waitsForYou)
    (self.prompts, fixedChanges, self.inbox) = (prompts, changes, inbox)
  }

  public var sent: [Intent] { state.withLock { $0.sent } }

  /// A fixture starts from a fresh session: no blocks.
  public func snapshot() -> [Block] { [] }

  public func nextPatches() async -> [TimelinePatch]? {
    await untilAnswered()
    let pulled: Int? = state.withLock { state in
      guard !state.closed, state.next < fixture.batches.count else { return nil }
      defer { state.next += 1 }
      if waitsForYou { state.waiting = fixture.batches[state.next].waiting }
      return state.next
    }
    guard let pulled else { return nil }
    for note in fixture.notes where note.batch == pulled {
      inbox.add(note.item)
      host?.notify(HostNote(note.item, badge: note.badge))
    }
    return fixture.batches[pulled]
  }

  public func send(_ intent: Intent) async throws -> (any SessionClient)? {
    if let call = intent.answers { inbox.answer(call) }
    let resume = state.withLock { state in
      state.sent.append(intent)
      if let call = intent.answers { state.waiting.remove(call) }
      return state.waiting.isEmpty ? state.parked.take() : nil
    }
    resume?.resume()
    return nil
  }

  /// The rows of the token's sigil whose insert holds the rest of the token in order, in list
  /// order: enough to drive a view, not the core's ranking.
  public func complete(_ token: String, limit: UInt32) -> [Completion] {
    guard let sigil = token.first, sigil == "@" || sigil == "/" else { return [] }
    let query = token.dropFirst().lowercased()
    let rows = completions.filter { row in
      guard row.insert.first == sigil else { return false }
      var rest = Substring(query)
      for character in row.insert.dropFirst().lowercased() where character == rest.first {
        rest = rest.dropFirst()
      }
      return rest.isEmpty
    }
    return Array(rows.prefix(Int(limit)))
  }

  public func history(limit: UInt32) -> [String] { Array(prompts.prefix(Int(limit))) }

  public func changes() async throws -> Changes { fixedChanges }

  public func close() {
    let resume = state.withLock { state in
      state.closed = true
      return state.parked.take()
    }
    resume?.resume()
  }

  private func untilAnswered() async {
    await withCheckedContinuation { (parked: CheckedContinuation<Void, Never>) in
      let ready = state.withLock { state in
        guard !state.waiting.isEmpty, !state.closed else { return true }
        state.parked = parked
        return false
      }
      if ready { parked.resume() }
    }
  }
}

extension [TimelinePatch] {
  /// The approvals and questions this batch leaves pending.
  var waiting: Set<String> {
    var calls: Set<String> = []
    for case .upsert(let block, _) in self {
      switch block.kind {
      case .approval(let call, _, _, _, _, let decision, _):
        if decision == nil { calls.insert(call) } else { calls.remove(call) }
      case .question(let call, _, _, let answer):
        if answer == nil { calls.insert(call) } else { calls.remove(call) }
      default: break
      }
    }
    return calls
  }
}

extension Intent {
  /// The approval or question this intent answers.
  var answers: String? {
    switch self {
    case .approve(let call, _): call
    case .answer(let question, _): question
    default: nil
    }
  }
}
