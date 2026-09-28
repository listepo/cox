// What the Rust core asks macOS to do (DT§4.4's `Host`), in `CoxClient`
// values: the seam between CoxPlatform, which implements it over the
// Keychain and AppKit, and CoxCore, which adapts it to the generated
// `AppHost`. Neither package depends on the other, so the Keychain side
// tests without the XCFramework and CoxCore never links AppKit.

/// Implemented by CoxPlatform's `MacHost`; called from Rust's threads.
public protocol PlatformHost: Sendable {
  /// The key stored for a provider section, `nil` when there is none.
  func secret(for section: String) -> String?
  /// A new inbox item for a notification and the Dock badge.
  func notify(_ note: HostNote)
  /// The Dock badge fell with no new item: an approval or question was
  /// answered, or its session closed.
  func badge(_ count: Int)
  /// An MCP server's login page or a link; the host decides what it opens.
  func open(_ url: String)
}

/// An inbox item reduced to what a notification shows.
public struct HostNote: Sendable, Equatable {
  public enum Kind: Sendable, Equatable {
    case approval
    case question
    case failed
    case taskDone(succeeded: Bool)
  }

  /// The session the item belongs to; groups its notifications.
  public var session: String
  public var kind: Kind
  /// The tool awaiting approval, the question, the error or the task label.
  public var text: String
  /// Items that block a turn, across all sessions.
  public var badge: Int
  /// The approval or question a notification action answers; `nil` for news.
  public var call: String?

  public init(session: String, kind: Kind, text: String, badge: Int, call: String? = nil) {
    (self.session, self.kind, self.text, self.badge, self.call) = (
      session, kind, text, badge, call
    )
  }
}
