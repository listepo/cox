// A reply's structure in the text (T37.23.8): a heading in the heading font, list
// and quote lines hanging past the markers `cox-render` sends as text, a table on
// tab stops, a rule as one character, and a streamed reply with the same paragraph
// styles a whole load gives it.

import AppKit
import CoxClient
import Testing

@testable import CoxTranscriptText

private func span(_ text: String, token: StyleToken = .text, bold: Bool = false) -> Span {
  var span = Span(text: text)
  (span.token, span.bold) = (token, bold)
  return span
}

/// A reply as `cox-render` lays it out: markers are the lines' first spans.
private let structured: [DocBlock] = [
  .text(kind: .heading(2), lines: [[span("## ", bold: true), span("Plan", bold: true)]]),
  .text(kind: .list, lines: [[span("• "), span("one")], [span("  • "), span("two")]]),
  .text(kind: .quote, lines: [[span("│ ", token: .dim), span("quoted")]]),
  .rule,
  .table(rows: [["k", "value"], ["key", "v"]]),
]

@MainActor private let style: TranscriptStyle = {
  var style = TranscriptStyle.system
  (style.heading, style.indent) = (.preferredFont(forTextStyle: .title3), 20)
  return style
}()

private func reply(_ blocks: [DocBlock]) -> Block {
  Block(id: "a", turn: 1, kind: .assistant(text: "", doc: StyledDoc(blocks: blocks)))
}

@MainActor
private func paragraph(_ view: TranscriptTextView, at text: String) -> NSParagraphStyle? {
  let location = (view.string as NSString).range(of: text).location
  return view.textStorage?.attribute(.paragraphStyle, at: location, effectiveRange: nil)
    as? NSParagraphStyle
}

@MainActor
struct TranscriptStructureTests {
  @Test func listAndQuoteLinesHangPastTheirMarkersAndAHeadingTakesItsFont() throws {
    let view = TranscriptTextView.make(style: style)
    view.load([reply(structured)])

    let font = view.textStorage?.attribute(.font, at: 3, effectiveRange: nil) as? NSFont
    #expect(font == style.heading)
    let one = try #require(paragraph(view, at: "• one"))
    let two = try #require(paragraph(view, at: "  • two"))
    #expect(one.firstLineHeadIndent == style.indent && one.headIndent > style.indent)
    #expect(two.headIndent > one.headIndent, "a deeper item hangs past its longer marker")
    let quote = try #require(paragraph(view, at: "│ quoted"))
    #expect(quote.firstLineHeadIndent == 0 && quote.headIndent > 0)
    #expect(try #require(paragraph(view, at: "k\tvalue")).tabStops.count == 1)
    #expect(view.string.contains("\n\u{FFFC}\n"), "a rule is one character on its own line")
  }

  @Test func aStreamedReplyHasTheParagraphStylesAWholeLoadGivesIt() {
    let streamed = TranscriptTextView.make(style: style)
    streamed.load([reply(Array(structured.prefix(2)))])
    streamed.apply(
      [.docTail(id: "a", from: 1, blocks: Array(structured.dropFirst()))], current: { _ in nil })
    let loaded = TranscriptTextView.make(style: style)
    loaded.load([reply(structured)])

    #expect(streamed.string == loaded.string)
    let styles = { (view: TranscriptTextView) in
      (0..<(view.string as NSString).length).map {
        view.textStorage?.attribute(.paragraphStyle, at: $0, effectiveRange: nil)
          as? NSParagraphStyle
      }
    }
    #expect(styles(streamed) == styles(loaded))
  }

  @Test func aDocCopiesAsMarkdownWithItsMarkersAsMarkdowns() {
    #expect(
      StyledDoc(blocks: structured).markdown
        == "## Plan\n\n- one\n  - two\n\n> quoted\n\n---\n\n| k | value |\n| --- | --- |\n| key | v |"
    )
  }
}
