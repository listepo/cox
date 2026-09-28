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
  /// Stops the pull; the session keeps running (DT§4.5).
  func close()
}

/// A recorded stream: what `record.rs` (cox-ffi) writes to
/// `desktop/macos/Fixtures/*.json`.
public struct Fixture: Equatable, Sendable, Decodable {
  /// The batches pulled from a session opened fresh, in order.
  public var batches: [[TimelinePatch]]
  /// The session's blocks once the last batch was pulled.
  public var snapshot: [Block]

  public init(batches: [[TimelinePatch]], snapshot: [Block]) {
    (self.batches, self.snapshot) = (batches, snapshot)
  }

  public init(contentsOf url: URL) throws {
    self = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
  }
}

public struct FixtureCoreClient: CoreClient {
  public let fixture: Fixture
  public let completions: [Completion]

  public init(fixture: Fixture, completions: [Completion] = []) {
    self.fixture = fixture
    self.completions = completions
  }

  public func open(_ request: OpenSession) async throws -> any SessionClient {
    FixtureSession(fixture: fixture, completions: completions)
  }
}

/// Hands out the recorded batches one pull at a time and keeps what was
/// sent, so a test can check the intents a store emitted. Completes from a
/// fixed list instead of the Rust completer.
public final class FixtureSession: SessionClient {
  public let id = "fixture"
  private let fixture: Fixture
  private let completions: [Completion]
  private let state = Mutex(State())

  private struct State {
    var next = 0
    var closed = false
    var sent: [Intent] = []
  }

  public init(fixture: Fixture, completions: [Completion] = []) {
    self.fixture = fixture
    self.completions = completions
  }

  public var sent: [Intent] { state.withLock { $0.sent } }

  /// A fixture starts from a fresh session: no blocks.
  public func snapshot() -> [Block] { [] }

  public func nextPatches() async -> [TimelinePatch]? {
    state.withLock { state in
      guard !state.closed, state.next < fixture.batches.count else { return nil }
      defer { state.next += 1 }
      return fixture.batches[state.next]
    }
  }

  public func send(_ intent: Intent) async throws -> (any SessionClient)? {
    state.withLock { $0.sent.append(intent) }
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

  public func close() { state.withLock { $0.closed = true } }
}
