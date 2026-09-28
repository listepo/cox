// A reply's structure (T37.23.8's Check): one reply with every doc block kind as
// `cox-render` lays it out — headings, prose, a bulleted and a numbered list with a
// nested item, a nested quote, a rule, a table and code — in light and dark, Solid,
// through the SwiftUI view the app hosts; and Copy as Markdown of it, whole and in
// part, gives its structure back as Markdown.

import AppKit
import CoxClient
import SnapshotTesting
import Testing

private func span(_ text: String, token: StyleToken = .text, bold: Bool = false) -> Span {
  var span = Span(text: text)
  (span.token, span.bold) = (token, bold)
  return span
}

private func heading(_ level: UInt8, _ text: String) -> DocBlock {
  let hashes = String(repeating: "#", count: Int(level)) + " "
  return .text(kind: .heading(level), lines: [[span(hashes, bold: true), span(text, bold: true)]])
}

private let wraps =
  "The watcher waits for its first event, so a slow disk no longer fails the test and the retry "
  + "loop that hid the race is gone."

/// Markers are the lines' first spans, as `cox-render` sends them.
private let doc = StyledDoc(blocks: [
  heading(1, "Release notes"),
  .text(kind: .paragraph, lines: [[span("The fix is small.")]]),
  heading(2, "What changed"),
  .text(kind: .list, lines: [[span("• "), span(wraps)], [span("  • "), span("nested item")]]),
  .text(kind: .list, lines: [[span("1. "), span("first")], [span("2. "), span("second")]]),
  heading(3, "Why"),
  .text(
    kind: .quote,
    lines: [
      [span("│ ", token: .dim), span("Flaky no more. " + wraps)],
      [span("│ │ ", token: .dim), span("nested quote")],
    ]),
  .rule,
  .table(rows: [["Crate", "Tests"], ["cox-core", "412"], ["cox-tui", "88"]]),
  .code(lang: "rust", lines: [[span("let event = rx.recv().await?;")]]),
])

private let markdown = """
  # Release notes

  The fix is small.

  ## What changed

  - \(wraps)
    - nested item

  1. first
  2. second

  ### Why

  > Flaky no more. \(wraps)
  > > nested quote

  ---

  | Crate | Tests |
  | --- | --- |
  | cox-core | 412 |
  | cox-tui | 88 |

  ```rust
  let event = rx.recv().await?;
  ```
  """

/// Streamed, so the reply has no source and copies from its doc.
private let reply = [Block(id: "a", turn: 1, kind: .assistant(text: "", doc: doc))]

// The reading column's width; the height follows the text.
// swiftlint:disable:next no_literal_size
private let size = NSSize(width: 760, height: 400)

@MainActor
@Suite(.serialized)
struct TranscriptReplyTests {
  @Test(arguments: [false, true])
  func replyWithEveryBlockKind(dark: Bool) throws {
    let host = Host(reply, size: size, dark: dark)
    defer { host.close() }
    host.fitToText()
    // Anti-aliasing differs slightly between machines; a real change moves far more pixels.
    assertSnapshot(
      of: try host.image(), as: .image(precision: 0.995, perceptualPrecision: 0.98),
      named: dark ? "dark-solid" : "light-solid", testName: "replyWithEveryBlockKind")
  }

  @Test func copyAsMarkdownGivesTheStructureBack() throws {
    let host = Host(reply, size: size)
    defer { host.close() }
    let text = host.text.string as NSString

    host.text.setSelectedRange(NSRange(location: 0, length: text.length))
    let whole = host.copy()
    #expect(whole.markdown == markdown)
    #expect(whole.plain?.contains("\u{FFFC}") == false, "a rule copies as no text")

    // From the first list's bullet to the quote's end: whole doc blocks copy as Markdown.
    let start = text.range(of: "• The").location
    let end = NSMaxRange(text.range(of: "nested quote"))
    host.text.setSelectedRange(NSRange(start..<end))
    let part = try #require(host.copy().markdown)
    let first = try #require(markdown.range(of: "- The"))
    let last = try #require(markdown.range(of: "nested quote"))
    #expect(part == String(markdown[first.lowerBound..<last.upperBound]))
  }
}
