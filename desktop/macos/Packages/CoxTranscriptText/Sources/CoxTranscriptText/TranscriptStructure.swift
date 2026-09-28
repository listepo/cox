// A reply's structure in the transcript text (T37.23.8, DT§5.2): headings, list
// items, quote lines, tables and rules stay text in the one text (A87), set by
// paragraph styles rather than drawn as flat paragraphs. `cox-render` sends a
// list's bullets, a quote's rails and a heading's `#` run as text, and they stay
// as it laid them out (DT-3): a wrapped line hangs past them. Its own file
// because it is the one place a doc block's kind becomes layout.

import AppKit
import CoxClient

extension NSAttributedString.Key {
  /// A doc line's `Paragraph`, on its first character.
  static let transcriptParagraph = NSAttributedString.Key("cox.transcript.paragraph")
}

/// A doc line's own paragraph style, and the same with the block spacing for
/// when it is its block's last paragraph (`TranscriptText.respace`).
final class Paragraph: NSObject {
  let own: NSParagraphStyle
  let last: NSParagraphStyle

  init(_ own: NSMutableParagraphStyle, spacing: CGFloat) {
    let last = own.mutableCopy() as? NSMutableParagraphStyle ?? NSMutableParagraphStyle()
    last.paragraphSpacing += spacing
    (self.own, self.last) = (own, last)
  }
}

/// A style's doc paragraphs, each made once: a heading's, and a hanging
/// line's per indent. A table's tab stops follow its cells, so each table
/// makes its own.
final class Paragraphs {
  private let style: TranscriptStyle
  private let heading: Paragraph
  private var hangs: [SIMD2<Double>: Paragraph] = [:]

  init(_ style: TranscriptStyle) {
    self.style = style
    let heading = NSMutableParagraphStyle()
    heading.paragraphSpacingBefore = style.blockSpacing
    self.heading = Paragraph(heading, spacing: style.blockSpacing)
  }

  /// A text line's paragraph: a heading's, or a list item's or quote line's
  /// hang past its marker, a list indented by the style's indent; `nil` for prose.
  func of(_ kind: TextKind, _ line: [Span], _ look: TextLook) -> Paragraph? {
    switch kind {
    case .paragraph: return nil
    case .heading: return heading
    case .list, .quote:
      let first = kind == .list ? style.indent : 0
      let marker = DocBlock.marker(line).map { marker in
        let font = line.first.map { look.look($0, .body).font } ?? style.body
        return (marker as NSString).size(withAttributes: [.font: font]).width.rounded(.up)
      }
      return hang(first, first + (marker ?? 0))
    }
  }

  private func hang(_ first: CGFloat, _ head: CGFloat) -> Paragraph {
    let key = SIMD2(Double(first), Double(head))
    if let made = hangs[key] { return made }
    let paragraph = NSMutableParagraphStyle()
    (paragraph.firstLineHeadIndent, paragraph.headIndent) = (first, head)
    let made = Paragraph(paragraph, spacing: style.blockSpacing)
    hangs[key] = made
    return made
  }

  /// A table's rows, their cells on tab stops past each column's widest cell.
  func table(_ rows: [[String]]) -> Paragraph {
    var widths: [CGFloat] = []
    for row in rows {
      for (column, cell) in row.enumerated() {
        let width = (cell as NSString).size(withAttributes: [.font: style.body]).width
        if column < widths.count {
          widths[column] = max(widths[column], width)
        } else {
          widths.append(width)
        }
      }
    }
    let paragraph = NSMutableParagraphStyle()
    var location: CGFloat = 0
    paragraph.tabStops = widths.dropLast().map { width in
      location += (width + style.indent).rounded(.up)
      return NSTextTab(textAlignment: .left, location: location)
    }
    return Paragraph(paragraph, spacing: style.blockSpacing)
  }
}

/// A doc rule: a thought's hairline across the text column, as one
/// attachment character, so it stays in the text and a drag runs over it.
final class RuleAttachment: NSTextAttachment {
  static let mark = "\u{FFFC}"
  private let thickness: CGFloat

  init(_ thought: TranscriptStyle.Thought) {
    thickness = thought.ruleWidth
    super.init(data: nil, ofType: nil)
    // Drawn at its bounds' size each time its line is, so the colour resolves in the view's
    // appearance. TextKit 2 draws an attachment's `image`, not `image(for:)`, without a view.
    let size = NSSize(width: thickness, height: thickness)
    image = NSImage(size: size, flipped: false) { [color = thought.rule] in
      color.setFill()
      $0.fill()
      return true
    }
  }

  required init?(coder: NSCoder) { nil }

  /// The full line width, at the middle of the line's lowercase letters.
  override func attachmentBounds(
    for attributes: [NSAttributedString.Key: Any], location: any NSTextLocation,
    textContainer: NSTextContainer?, proposedLineFragment: CGRect, position: CGPoint
  ) -> CGRect {
    let padding = textContainer?.lineFragmentPadding ?? 0
    let middle = ((attributes[.font] as? NSFont)?.xHeight ?? 0) / 2
    return CGRect(
      x: 0, y: middle, width: max(0, proposedLineFragment.width - 2 * padding), height: thickness)
  }
}

extension TranscriptText {
  /// `respace` for a block with structure: each paragraph in `spaced` takes
  /// its line's own style, the one at `last` its spaced one, and only one
  /// whose style is wrong changes.
  static func restyle(
    _ text: NSMutableAttributedString, _ spaced: NSRange, last: Int, _ look: TextLook
  ) {
    let string = text.mutableString
    var location = spaced.location
    while location < NSMaxRange(spaced) {
      let paragraph = string.paragraphRange(for: NSRange(location: location, length: 0))
      let own =
        text.attribute(.transcriptParagraph, at: paragraph.location, effectiveRange: nil)
        as? Paragraph
      let want = paragraph.location == last ? own?.last ?? look.spacing : own?.own
      text.enumerateAttribute(.paragraphStyle, in: paragraph) { value, range, _ in
        guard (value as? NSParagraphStyle) != want else { return }
        if let want {
          text.addAttribute(.paragraphStyle, value: want, range: range)
        } else {
          text.removeAttribute(.paragraphStyle, range: range)
        }
      }
      location = NSMaxRange(paragraph)
    }
  }
}
