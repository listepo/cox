// The transcript as one attributed string (T37.40): timeline blocks → text and
// the `BlockRanges` over it. Each block becomes one piece; pieces are joined
// and tracked here only, so styled spans from Rust (T37.43) and cards as
// attachments (T37.41) change a kind's piece without touching the tracking.

import AppKit
import CoxClient

/// What the transcript text is drawn with. This package holds no design
/// values: the app builds a style from CoxUI's tokens (T37.23), and `system`
/// is the platform's own body text for tests and previews.
public struct TranscriptStyle: Equatable {
  public var body: NSFont
  public var code: NSFont
  public var text: NSColor
  /// The space after a block's last line.
  public var blockSpacing: CGFloat
  /// The space between the text and the view's edges.
  public var inset: NSSize

  public init(body: NSFont, code: NSFont, text: NSColor, blockSpacing: CGFloat, inset: NSSize) {
    (self.body, self.code, self.text) = (body, code, text)
    (self.blockSpacing, self.inset) = (blockSpacing, inset)
  }

  public static var system: TranscriptStyle {
    let body = NSFont.preferredFont(forTextStyle: .body)
    let code =
      body.fontDescriptor.withDesign(.monospaced)
      .flatMap { NSFont(descriptor: $0, size: body.pointSize) } ?? body
    return TranscriptStyle(body: body, code: code, text: .textColor, blockSpacing: 0, inset: .zero)
  }
}

enum TranscriptText {
  /// Between two blocks and between the lines of one.
  static let separator = "\n"

  typealias Built = (text: NSAttributedString, ranges: BlockRanges)

  /// The whole transcript. A block id seen twice keeps its first block, as
  /// `SessionStore` holds one block per id.
  static func build(_ blocks: some Sequence<Block>, style: TranscriptStyle) -> Built {
    let out = NSMutableAttributedString()
    var ranges = BlockRanges()
    for block in blocks where ranges.index(of: block.id) == nil {
      let piece = piece(block.kind, style: style)
      if piece.length > 0, out.length > 0 {
        // The previous block's attributes, so its last paragraph keeps its spacing.
        let attributes = out.attributes(at: out.length - 1, effectiveRange: nil)
        out.append(NSAttributedString(string: separator, attributes: attributes))
      }
      ranges.append(block.id, NSRange(location: out.length, length: piece.length))
      out.append(piece)
    }
    return (out, ranges)
  }

  static func piece(_ kind: BlockKind, style: TranscriptStyle) -> NSAttributedString {
    let out = NSMutableAttributedString()
    for run in runs(kind) where !run.text.isEmpty {
      let text = (out.length > 0 ? separator : "") + run.text
      let font = run.code ? style.code : style.body
      out.append(
        NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: style.text]))
    }
    guard out.length > 0 else { return out }
    let spacing = NSMutableParagraphStyle()
    spacing.paragraphSpacing = style.blockSpacing
    let last = (out.string as NSString).paragraphRange(
      for: NSRange(location: out.length - 1, length: 0))
    out.addAttribute(.paragraphStyle, value: spacing, range: last)
    return out
  }

  /// The text a block shows, as body or code runs. A turn's meta line shows
  /// on hover (DT§5.2), so it has no text.
  static func runs(_ kind: BlockKind) -> [(text: String, code: Bool)] {
    switch kind {
    case .user(let text, _), .thinking(let text), .notice(_, let text), .error(let text, _):
      return [(text, false)]
    case .assistant(_, let doc):
      return doc.blocks.compactMap(run)
    case .tool(_, let summary, _, _, _, _, _, _, _), .toolGroup(let summary, _, _),
      .approval(_, _, let summary, _, _, _, _):
      return [(summary, false)]
    case .question(_, let question, _, _):
      return [(question, false)]
    case .task(_, let label, _, _, _, _):
      return [(label, false)]
    case .compaction(_, _, _, let summary):
      return summary.map { [($0, false)] } ?? []
    case .checkpoint(let files):
      return [(files.joined(separator: separator), false)]
    case .turnMeta:
      return []
    }
  }

  static func run(_ block: DocBlock) -> (text: String, code: Bool)? {
    func joined(_ lines: [[Span]]) -> String {
      lines.map { $0.map(\.text).joined() }.joined(separator: separator)
    }
    switch block {
    case .text(_, let lines): return (joined(lines), false)
    case .code(_, let lines): return (joined(lines), true)
    case .table(let rows):
      return (rows.map { $0.joined(separator: "\t") }.joined(separator: separator), false)
    case .rule: return nil
    }
  }
}
