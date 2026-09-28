// What the toolbar's model popover offers and which providers can answer (DT§5.1, T37.22.6,
// A110), field for field as cox-ffi exports `cox_app::models`. Separate from the session seam
// because both are read from the config for a cwd, before or beside any open session.

/// `cox_app::ModelChoice`: one model a tier can switch to, as `/model <tier> <id>` names it.
public struct ModelChoice: Equatable, Sendable {
  public var tier: Tier
  /// The `[providers.<name>]` section the tier calls.
  public var provider: String
  /// The id sent on the wire, and what the popover shows: the catalog has no display names.
  public var id: String
  /// The efforts it takes; empty means any.
  public var efforts: [Effort]
  public var contextWindow: UInt32?

  public init(
    tier: Tier, provider: String, id: String, efforts: [Effort] = [], contextWindow: UInt32? = nil
  ) {
    (self.tier, self.provider, self.id) = (tier, provider, id)
    (self.efforts, self.contextWindow) = (efforts, contextWindow)
  }
}

/// The catalog half of cox-ffi's `App`.
public protocol ModelsClient: Sendable {
  /// Each tier's models for a session in `cwd`, the tier's configured one first.
  func models(cwd: String) throws -> [ModelChoice]
  /// The provider sections a turn in `cwd` could run on now: a key found or a local server
  /// listening. Probes the servers, so it waits.
  func usableProviders(cwd: String) async throws -> [String]
}

/// A fixed catalog and provider list: enough to drive the popover and footer in a test.
public struct FixtureModels: ModelsClient {
  public var fixedModels: [ModelChoice]
  public var fixedProviders: [String]

  public init(models: [ModelChoice] = [], providers: [String] = []) {
    (fixedModels, fixedProviders) = (models, providers)
  }

  public func models(cwd: String) -> [ModelChoice] { fixedModels }
  public func usableProviders(cwd: String) async -> [String] { fixedProviders }
}
