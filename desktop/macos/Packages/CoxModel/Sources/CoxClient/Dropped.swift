// Project values the guard list threw out (T37.30.4, DT§5.7), field for field as cox-ffi exports
// `cox_app::Dropped`. Separate from `Settings.swift` because a dropped value is no setting in
// effect: it is what the project's `.cox/config.toml` asked for and did not get (`plan.md` §1.6).

/// A value the project set that the guard list reverted, with why.
public struct Dropped: Identifiable, Equatable, Sendable {
  /// Dotted; `mcp.servers.*.sandbox` names the servers in `value`.
  public var key: String
  /// What the project set.
  public var value: String
  /// What is in effect instead.
  public var kept: String
  public var reason: String

  public var id: String { key }

  public init(key: String, value: String, kept: String, reason: String) {
    (self.key, self.value, self.kept, self.reason) = (key, value, kept, reason)
  }
}
