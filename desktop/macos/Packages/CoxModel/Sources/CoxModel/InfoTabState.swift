// The inspector's Info tab (T37.29.5, DT§5.1): cox-app's `Info` as the rows CoxUI's
// `InfoTab.State` holds — the session's facts and the config layers it runs with. Here, not in
// CoxUI, because these decide what the tab shows (DS§1); the app copies them into
// `InfoTab.State` field for field.

import CoxClient
import Foundation

public struct InfoTabState: Equatable, Sendable {
  /// `KeyValueGrid.Row`.
  public struct Fact: Equatable, Sendable {
    public var label: String
    public var values: [String]
    public var isDetail = false
  }

  /// Id, folder, worktree and its branch, rollout.
  public var session: [Fact] = []
  /// A row per layer with its key count, its file under it as a detail row.
  public var config: [Fact] = []

  public init() {}

  /// `home` is shortened to `~` in every path.
  public init(_ info: Info, home: String = NSHomeDirectory()) {
    let path = { (full: String) in
      full == home || full.hasPrefix(home + "/") ? "~" + full.dropFirst(home.count) : full
    }
    session = [
      Fact(label: "Session", values: [info.session]),
      Fact(label: "Folder", values: [path(info.cwd)]),
    ]
    if let tree = info.worktree {
      session.append(Fact(label: "Worktree", values: [path(tree.path)]))
      session.append(Fact(label: "Branch", values: [tree.branch ?? "detached"], isDetail: true))
    }
    session.append(Fact(label: "Rollout", values: [path(info.rollout)]))
    for source in info.config {
      let keys = "\(source.keys) \(source.keys == 1 ? "key" : "keys")"
      config.append(Fact(label: source.layer.rawValue, values: [keys]))
      if let file = source.file {
        config.append(Fact(label: path(file), values: [], isDetail: true))
      }
    }
  }
}

extension SessionStore {
  /// The Info tab's state, read from the core when the tab asks (T37.29.5).
  public func infoTab() async throws -> InfoTabState {
    InfoTabState(try await session.info())
  }
}
