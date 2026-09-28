// `PreviewState` fixtures for the tool card's molecules (T37.21.1, T37.21.4): tool headers and
// terminal tails, as mockup screen 28's transcript shows them. Separate from `PreviewState.swift`
// so molecules built in parallel add their fixtures without editing one file.

import SwiftUI

extension PreviewState {
  /// A finished edit with its line counts, the mockup's first tool card.
  static let toolEdited = ToolHeader.Item(
    tile: .edit, symbol: symbol(.edit), verb: "Edited",
    subject: "crates/cox-provider-http/src/retry.rs", change: .init(added: 18, removed: 4),
    state: .succeeded, duration: "0.1 s")

  /// A command still running, its subject monospaced.
  static let toolRunning = ToolHeader.Item(
    tile: .shell, symbol: symbol(.shell), verb: "Running",
    subject: "cargo nextest run -p cox-provider-http", subjectIsCode: true, state: .running,
    duration: "12 s")

  /// A search over several files, with a secondary detail.
  static let toolExplored = ToolHeader.Item(
    tile: .search, symbol: symbol(.search), verb: "Explored", subject: "6 files",
    detail: "· retry.rs, http.rs, sse.rs, lib.rs +2", state: .succeeded, duration: "1.1 s")

  /// A risky call that failed.
  static let toolFailed = ToolHeader.Item(
    tile: .shell, symbol: symbol(.shell), verb: "Ran", subject: "git push origin wt/retry-jitter",
    subjectIsCode: true, risk: .init(text: risk, level: .high), state: .failed,
    duration: "2.4 s")
}

/// A tool header across the reading column.
struct ToolHeaderSample: View {
  let item: ToolHeader.Item
  let isExpanded: Bool?

  init(_ item: ToolHeader.Item, isExpanded: Bool? = nil) {
    self.item = item
    self.isExpanded = isExpanded
  }

  var body: some View {
    ToolHeader(item, isExpanded: isExpanded).frame(width: Size.readingWidth)
  }
}
