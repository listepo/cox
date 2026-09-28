// A prompt's bubble and a thought's fold in the transcript text (T37.23.4,
// DT§5.2): both stay text, so a drag can start partway through a prompt and run
// on into the reply, which a card attachment (selected whole) cannot give. A
// `Decor` attribute marks their paragraphs; a layout fragment draws the bubble
// or the rule behind them. A thought's open or folded state is the view's own
// (`openThoughts`): folding edits only the text after its header, so no other
// block's range moves. Its own file because it is the one place that draws
// behind the text.

import AppKit
import CoxClient

extension TranscriptStyle {
  /// A user prompt's face behind its text and its attachments' tiles.
  public struct Bubble: Equatable {
    public var fill: NSColor
    public var radius: CGFloat
    /// From the bubble's edge to its text.
    public var padding: NSSize
    /// Between two attachment tiles.
    public var gap: CGFloat

    public init(fill: NSColor, radius: CGFloat, padding: NSSize, gap: CGFloat) {
      (self.fill, self.radius, self.padding, self.gap) = (fill, radius, padding, gap)
    }

    public static var system: Bubble {
      Bubble(fill: .quaternarySystemFill, radius: 0, padding: .zero, gap: 0)
    }
  }

  /// A thought's reasoning and the rule beside it.
  public struct Thought: Equatable {
    public var font: NSFont
    public var color: NSColor
    public var rule: NSColor
    public var ruleWidth: CGFloat
    /// From the rule to the reasoning.
    public var indent: CGFloat

    public init(font: NSFont, color: NSColor, rule: NSColor, ruleWidth: CGFloat, indent: CGFloat) {
      (self.font, self.color, self.rule) = (font, color, rule)
      (self.ruleWidth, self.indent) = (ruleWidth, indent)
    }

    public static var system: Thought {
      Thought(
        font: .preferredFont(forTextStyle: .body), color: .secondaryLabelColor,
        rule: .separatorColor,
        ruleWidth: 1, indent: 0)
    }
  }
}

extension NSAttributedString.Key {
  /// The `Decor` of a prompt's or a thought's characters.
  static let transcriptDecor = NSAttributedString.Key("cox.transcript.decor")
  /// A decorated paragraph's `Decor.Edge`, as its raw value.
  static let transcriptEdge = NSAttributedString.Key("cox.transcript.edge")
}

/// How a prompt's or a thought's paragraphs are set and what is drawn behind
/// them. One per style and kind (`TextLook`).
final class Decor: NSObject {
  enum Kind { case bubble, thought }

  /// Whether a paragraph is its block's first, last, both or neither.
  struct Edge: OptionSet {
    let rawValue: Int
    static let first = Edge(rawValue: 1)
    static let last = Edge(rawValue: 2)
  }

  let kind: Kind
  private let bubble: TranscriptStyle.Bubble
  private let thought: TranscriptStyle.Thought
  /// One paragraph style per `Edge` raw value.
  private var styles: [NSParagraphStyle] = []

  init(_ kind: Kind, _ style: TranscriptStyle) {
    (self.kind, bubble, thought) = (kind, style.bubble, style.thought)
    super.init()
    styles = (0..<4).map { raw in
      let edge = Edge(rawValue: raw)
      let paragraph = NSMutableParagraphStyle()
      let spacing = edge.contains(.last) ? style.blockSpacing : 0
      switch kind {
      case .bubble:
        let side = bubble.padding.width
        (paragraph.firstLineHeadIndent, paragraph.headIndent, paragraph.tailIndent) = (
          side, side, -side
        )
        paragraph.paragraphSpacingBefore = edge.contains(.first) ? bubble.padding.height : 0
        paragraph.paragraphSpacing = spacing + (edge.contains(.last) ? bubble.padding.height : 0)
      case .thought:
        // The first paragraph is the fold header, at the margin; the reasoning sits past the rule.
        let indent = edge.contains(.first) ? 0 : thought.indent
        (paragraph.firstLineHeadIndent, paragraph.headIndent) = (indent, indent)
        paragraph.paragraphSpacing = spacing
      }
      return paragraph
    }
  }

  /// `TranscriptText.respace` for a decorated block: each paragraph from
  /// `from`'s on takes its edge's style, and only one whose style is wrong changes.
  func respace(_ text: NSMutableAttributedString, block: NSRange, from: Int) {
    let string = text.mutableString
    let end = NSMaxRange(block)
    let start = min(max(from, block.location), end - 1)
    var location = string.paragraphRange(for: NSRange(location: start, length: 0)).location
    while location < end {
      let paragraph = string.paragraphRange(for: NSRange(location: location, length: 0))
      var edge: Edge = paragraph.location <= block.location ? .first : []
      if NSMaxRange(paragraph) >= end { edge.insert(.last) }
      let style = styles[edge.rawValue]
      text.enumerateAttribute(.paragraphStyle, in: paragraph) { value, range, _ in
        guard (value as? NSParagraphStyle) !== style else { return }
        text.addAttributes([.paragraphStyle: style, .transcriptEdge: edge.rawValue], range: range)
      }
      location = NSMaxRange(paragraph)
    }
  }

  /// What a paragraph at `edge` draws behind its lines, in its fragment's
  /// coordinates: its slice of the bubble, or the rule beside the reasoning.
  func area(_ fragment: NSTextLayoutFragment, _ edge: Edge) -> CGRect? {
    guard let first = fragment.textLineFragments.first?.typographicBounds,
      let last = fragment.textLineFragments.last?.typographicBounds
    else { return nil }
    let container = fragment.textLayoutManager?.textContainer
    // The fragment's frame starts at its text, past the paragraph's indent: the
    // bubble and the rule start at the text column's edge, as unindented text does.
    let frame = fragment.layoutFragmentFrame
    let margin = (container?.lineFragmentPadding ?? 0) - frame.minX
    let height = frame.height
    switch kind {
    case .bubble:
      let width = (container?.size.width ?? frame.width) - 2 * (container?.lineFragmentPadding ?? 0)
      let top = edge.contains(.first) ? first.minY - bubble.padding.height : 0
      let bottom = edge.contains(.last) ? last.maxY + bubble.padding.height : height
      return CGRect(x: margin, y: top, width: width, height: bottom - top)
    case .thought:
      guard !edge.contains(.first) else { return nil }
      let bottom = edge.contains(.last) ? last.maxY : height
      return CGRect(x: margin, y: 0, width: thought.ruleWidth, height: bottom)
    }
  }

  func draw(_ rect: CGRect, _ edge: Edge, in context: CGContext) {
    context.saveGState()
    defer { context.restoreGState() }
    switch kind {
    case .bubble:
      // Only the bubble's own ends are round: a slice runs on past a cut edge and is clipped there.
      var shape = rect
      if !edge.contains(.first) {
        (shape.origin.y, shape.size.height) = (
          rect.minY - bubble.radius, shape.height + bubble.radius
        )
      }
      if !edge.contains(.last) { shape.size.height += bubble.radius }
      let radius = min(bubble.radius, shape.width / 2, shape.height / 2)
      context.clip(to: rect)
      context.addPath(
        CGPath(roundedRect: shape, cornerWidth: radius, cornerHeight: radius, transform: nil))
      context.setFillColor(bubble.fill.cgColor)
      context.fillPath()
    case .thought:
      context.setFillColor(thought.rule.cgColor)
      context.fill(rect)
    }
  }
}

/// A decorated paragraph's layout: the bubble or the rule under its text.
final class DecorFragment: NSTextLayoutFragment {
  private var decoration: (decor: Decor, edge: Decor.Edge)? {
    guard let text = (textElement as? NSTextParagraph)?.attributedString, text.length > 0,
      let decor = text.attribute(.transcriptDecor, at: 0, effectiveRange: nil) as? Decor
    else { return nil }
    let raw = text.attribute(.transcriptEdge, at: 0, effectiveRange: nil) as? Int
    return (decor, Decor.Edge(rawValue: raw ?? 0))
  }

  override var renderingSurfaceBounds: CGRect {
    let bounds = super.renderingSurfaceBounds
    guard let (decor, edge) = decoration, let area = decor.area(self, edge) else { return bounds }
    return bounds.union(area)
  }

  override func draw(at point: CGPoint, in context: CGContext) {
    if let (decor, edge) = decoration, let area = decor.area(self, edge) {
      decor.draw(area.offsetBy(dx: point.x, dy: point.y), edge, in: context)
    }
    super.draw(at: point, in: context)
  }
}

/// Gives a decorated paragraph its `DecorFragment` and a quote line its
/// `QuoteFragment` (`TranscriptStructure.swift`); holds no state, so every
/// transcript shares one.
final class DecorLayout: NSObject, NSTextLayoutManagerDelegate {
  @MainActor static let shared = DecorLayout()

  func textLayoutManager(
    _ textLayoutManager: NSTextLayoutManager, textLayoutFragmentFor location: any NSTextLocation,
    in textElement: NSTextElement
  ) -> NSTextLayoutFragment {
    let text = (textElement as? NSTextParagraph)?.attributedString
    let decorated =
      text.map {
        $0.length > 0 && $0.attribute(.transcriptDecor, at: 0, effectiveRange: nil) != nil
      }
      ?? false
    let range = textElement.elementRange
    if decorated { return DecorFragment(textElement: textElement, range: range) }
    return QuoteFragment.quoted(text)
      ? QuoteFragment(textElement: textElement, range: range)
      : NSTextLayoutFragment(textElement: textElement, range: range)
  }
}

extension TranscriptText {
  /// A prompt, with its attachments' tiles on a line under it; a thought, its
  /// fold header and, open, its reasoning under it (none while it has no text).
  /// `nil` for any other block.
  @MainActor
  static func decorated(
    _ block: Block, _ look: TextLook, cards: TranscriptCards
  ) -> NSMutableAttributedString? {
    let out = NSMutableAttributedString()
    switch block.kind {
    case .user(let text, let attachments):
      out.append(NSAttributedString(string: text, attributes: look.prompt))
      if !attachments.isEmpty, out.length > 0 {
        out.append(NSAttributedString(string: separator, attributes: look.prompt))
      }
      for name in attachments {
        let tile = CardAttachment(block, cards: cards, role: .thumbnail(name))
        var attributes = look.prompt
        (attributes[.attachment], attributes[.kern]) = (tile, look.style.bubble.gap)
        out.append(NSAttributedString(string: "\u{FFFC}", attributes: attributes))
      }
    case .thinking(let text):
      guard !text.isEmpty else { break }
      let header = CardAttachment(block, cards: cards, role: .header)
      out.append(NSAttributedString(attachment: header))
      if cards.isOpen(block.id) { out.append(NSAttributedString(string: separator + text)) }
      out.addAttributes(look.thought, range: NSRange(location: 0, length: out.length))
    default:
      return nil
    }
    return out
  }
}

extension TranscriptTextView {
  /// Opens or folds a thought. Only the text after its header changes, so
  /// every other block keeps its range and the header keeps its view.
  public func setThought(_ id: BlockID, open: Bool) {
    guard openThoughts.contains(id) != open else { return }
    if open { openThoughts.insert(id) } else { openThoughts.remove(id) }
    guard let index = blockRanges.index(of: id), let block = blocks[id],
      case .thinking(let text) = block.kind
    else { return }
    let range = blockRanges.ranges[index]
    guard range.length > 0 else { return }
    let look = TextLook.of(style)
    let tail =
      open
      ? NSAttributedString(string: TranscriptText.separator + text, attributes: look.thought)
      : NSAttributedString()
    splice(index, NSRange(location: 1, length: range.length - 1), with: tail, look)
    let header = textStorage?.attribute(.attachment, at: range.location, effectiveRange: nil)
    (header as? CardAttachment)?.update(block)
  }

  /// A thought gained `text`: its first text brings the header; an open one
  /// shows the text at its end, a folded one only keeps it.
  func thoughtGrew(_ index: Int, by text: String, to block: Block, _ look: TextLook) {
    blocks[block.id] = block
    let length = blockRanges.ranges[index].length
    if length == 0 {
      let piece = TranscriptText.piece(block, look, cards: hostedCards)
      splice(index, NSRange(location: 0, length: 0), with: piece.text, spaced: true, look)
    } else if openThoughts.contains(block.id) {
      let grown = NSAttributedString(string: text, attributes: look.thought)
      splice(index, NSRange(location: length, length: 0), with: grown, look)
    }
  }
}
