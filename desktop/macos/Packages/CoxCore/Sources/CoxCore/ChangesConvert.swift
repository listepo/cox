// The Changes tab's records from cox-ffi into CoxClient's (T37.29.1), apart from `Convert.swift`'s
// timeline so each file stays one concern. Field for field; nothing is decided here.

import CoxClient
import CoxFFIBindings

extension CoxClient.Changes {
  init(_ changes: CoxFFIBindings.Changes) {
    self.init(
      files: changes.files.map { CoxClient.ChangedFile($0) },
      checkpoints: changes.checkpoints.map {
        CoxClient.Checkpoint(turn: $0.turn, label: $0.label, time: $0.time)
      },
      worktree: changes.worktree.map { CoxClient.Linked($0) })
  }
}

extension CoxClient.Linked {
  /// Shared by the Changes and Info tabs, which both show the worktree.
  init(_ linked: CoxFFIBindings.Linked) {
    self.init(
      path: linked.path, branch: linked.branch, base: linked.base, commit: linked.commit,
      bytes: linked.bytes)
  }
}

extension CoxClient.ChangedFile {
  init(_ file: CoxFFIBindings.ChangedFile) {
    let change: CoxClient.FileChange =
      switch file.change {
      case .edited: .edited
      case .created: .created
      case .deleted: .deleted
      }
    self.init(
      path: file.path, change: change, added: file.added, removed: file.removed, call: file.call,
      turn: file.turn)
  }
}
