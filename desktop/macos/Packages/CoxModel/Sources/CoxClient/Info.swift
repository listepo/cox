// What the inspector's Info tab lists (T37.29.5, DT§5.1), field for field as cox-ffi exports
// `cox_app::Info`: the session's id, cwd, linked worktree, the config layers it runs with and its
// rollout file. Separate from the timeline because it answers a call, not a patch.

public struct Info: Equatable, Sendable {
  public var session: String
  public var cwd: String
  /// `nil` when the session runs outside a linked worktree.
  public var worktree: Linked?
  /// Each layer that set at least one key, in load order.
  public var config: [ConfigSource]
  /// The session's JSONL rollout.
  public var rollout: String

  public init(
    session: String = "", cwd: String = "", worktree: Linked? = nil, config: [ConfigSource] = [],
    rollout: String = ""
  ) {
    (self.session, self.cwd, self.worktree, self.config, self.rollout) = (
      session, cwd, worktree, config, rollout
    )
  }
}

/// One config layer the session's config came from.
public struct ConfigSource: Equatable, Sendable {
  public var layer: Layer
  /// The file it was read from; `nil` for a layer that is not a file.
  public var file: String?
  /// How many effective leaves it set.
  public var keys: UInt32

  public init(layer: Layer, file: String? = nil, keys: UInt32) {
    (self.layer, self.file, self.keys) = (layer, file, keys)
  }
}
