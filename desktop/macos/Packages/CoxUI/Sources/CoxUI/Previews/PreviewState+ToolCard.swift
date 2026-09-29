// `PreviewState` fixtures for `ToolCard` (T37.23): the mockup's edit with its diff, the running
// and failed commands with their tails, and an exploration with no detail, built from the tool
// molecules' fixtures. Separate so the organism's fixtures do not edit the molecules' files.

import SwiftUI

extension PreviewState {
  /// The mockup's first tool card: the retry edit and its hunk.
  static let cardEdited = ToolCard.Content(
    header: toolEdited, detail: .diff([.init(header: hunkHeader, lines: hunk)]))

  /// `cargo nextest` still printing.
  static let cardRunning = ToolCard.Content(
    header: toolRunning, detail: .tail(tail, exit: .running))

  /// The risky push that failed, with the lines it printed.
  static let cardFailed = ToolCard.Content(
    header: toolFailed, detail: .tail(Array(tail.prefix(2)), exit: tailFailed))

  /// A read-only search: a header and nothing to open.
  static let cardExplored = ToolCard.Content(header: toolExplored)

  /// A finished call whose body is a plugin's own tree (T52.23.2).
  static let cardPlugin = ToolCard.Content(header: toolExplored, detail: .plugin(pluginKeyValue))
}

/// A tool card across the reading column.
struct ToolCardSample: View {
  let content: ToolCard.Content
  let isExpanded: Bool

  init(_ content: ToolCard.Content, isExpanded: Bool = false) {
    self.content = content
    self.isExpanded = isExpanded
  }

  var body: some View {
    ToolCard(content, isExpanded: isExpanded).frame(width: Size.readingWidth)
  }
}
