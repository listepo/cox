// A reply's structure in the text (T37.23.8, A92): a heading in the heading font
// without its `#` run, a list item's marker in the gutter before its text, a quote
// line past a bar per quote, a table on tab stops, a rule as one character, and a
// streamed reply with the same paragraph styles a whole load gives it.

import AppKit
import CoxClient
import Testing

@testable import CoxTranscriptText

private func span(_ text: String, token: StyleToken = .text, bold: Bool = false) -> Span {
  var span = Span(text: text)
  (span.token, span.bold) = (token, bold)
  return span
}

/// A reply as `cox-render` sends it: level, depth and marker apart from the text.
private let structured: [DocBlock] = [
  .text(kind: .heading(2), lines: [TextLine([span("Plan", bold: true)])]),
  .text(
    kind: .list,
    lines: [TextLine([span("one")], marker: "•"), TextLine([span("two")], depth: 1, marker: "•")]),
  .text(kind: .quote, lines: [TextLine([span("quoted")], quote: 1)]),
  .text(kind: .quote, lines: [TextLine([span("deeper")], quote: 2)]),
  .rule,
  .table(rows: [["k", "value"], ["key", "v"]]),
]

@MainActor private let style: TranscriptStyle = {
  var style = TranscriptStyle.system
  (style.heading, style.indent, style.thought.indent) = (
    .preferredFont(forTextStyle: .title3), 20, 12
  )
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
  @Test func markersSitInTheGutterQuotesPastTheirBarsAndHeadingsInTheirFont() throws {
    let view = TranscriptTextView.make(style: style)
    view.load([reply(structured)])

    #expect(view.string.hasPrefix("Plan\n"), "a heading shows without its `#` run")
    let font = view.textStorage?.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
    #expect(font == style.heading)
    let one = try #require(paragraph(view, at: "\t•\tone"))
    #expect(one.firstLineHeadIndent == 0 && one.headIndent == style.indent)
    #expect(one.tabStops.map(\.location).last == style.indent, "the text starts past the gutter")
    let two = try #require(paragraph(view, at: "\t•\ttwo"))
    #expect(two.firstLineHeadIndent == style.indent && two.headIndent == 2 * style.indent)
    let quote = try #require(paragraph(view, at: "quoted"))
    #expect(quote.firstLineHeadIndent == 12 && quote.headIndent == 12)
    #expect(try #require(paragraph(view, at: "deeper")).headIndent == 24, "a bar per quote")
    #expect(!view.string.contains("│") && !view.string.contains("#"))
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
        == "## Plan\n\n- one\n  - two\n\n> quoted\n\n> > deeper\n\n---\n\n"
        + "| k | value |\n| --- | --- |\n| key | v |"
    )
  }

  @Test func aQuoteLineLaysOutWithItsBarsAndProseWithout() throws {
    let view = TranscriptTextView.make(style: style)
    view.load([reply(structured)])
    let manager = try #require(view.textLayoutManager)
    manager.ensureLayout(for: manager.documentRange)
    var quoted: [String] = []
    manager.enumerateTextLayoutFragments(from: manager.documentRange.location) { fragment in
      let text = (fragment.textElement as? NSTextParagraph)?.attributedString.string
      if fragment is QuoteFragment, let text {
        quoted.append(text.trimmingCharacters(in: .newlines))
      }
      return true
    }
    #expect(quoted == ["quoted", "deeper"])
  }
}
