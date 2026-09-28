// `ChangedFileRow` (DS§6.3 row `ChangedFileRow`, the inspector's Changes tab `.fr`): one file
// the session changed — edited, created or deleted — with the lines it gained and lost and what you can
// do to it. Separate so the Changes tab and the review list draw a changed file the same way.

import SwiftUI

/// An `InspectorRow`: the change's glyph, the path with its directory in `text.secondary` and
/// the name kept visible when it truncates, then a `DiffStat`.
struct ChangedFileRow: View {
  /// How the session changed a file.
  enum Change: Sendable {
    case edited, created, deleted
  }

  /// A changed file, as the core reports it.
  struct File: Equatable, Sendable {
    /// Relative to the workspace root, `cox-provider-http/src/retry.rs`.
    var path: String
    var change: Change
    var added: Int
    var removed: Int
  }

  let file: File
  let isSelected: Bool
  let actions: [RowAction]

  init(_ file: File, isSelected: Bool = false, actions: [RowAction] = []) {
    self.file = file
    self.isSelected = isSelected
    self.actions = actions
  }

  var body: some View {
    InspectorRow(symbol: file.change.symbol, isSelected: isSelected, actions: actions) {
      Text(file.styledPath)
        .truncationMode(.head)
        .frame(maxWidth: .infinity, alignment: .leading)
      DiffStat(added: file.added, removed: file.removed)
    }
  }
}

extension ChangedFileRow.File {
  /// The directory, up to its last `/`, dimmed; the file name in the row's colour.
  var styledPath: AttributedString {
    let cut = path.lastIndex(of: "/").map(path.index(after:)) ?? path.startIndex
    var directory = AttributedString(String(path[..<cut]))
    directory.foregroundColor = Color(.textSecondary)
    return directory + AttributedString(String(path[cut...]))
  }
}

extension ChangedFileRow.Change {
  /// The DS§3.7 symbol: a pencil for an edit, a document for a new file, a bin for a removed one.
  var symbol: String {
    switch self {
    case .edited: "pencil"
    case .created: "doc.text"
    case .deleted: "trash"
    }
  }
}

#Preview("selected") { PreviewMatrix { ChangedFileSample(isSelected: true) } }
#Preview("list") { PreviewMatrix { ChangedFileList() } }
#Preview("deleted") { PreviewMatrix { DeletedFileSample() } }
