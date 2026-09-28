// A reply's `StyledDoc` back as Markdown (T37.42, T37.42.1): a heading's level,
// a list item's marker and a quote's depth, which the doc carries apart from the
// text (A92), become Markdown's own `#`, `-` or number and `>`. A streamed
// `docTail` carries doc blocks, not source, so this is the one rendering the
// session store keeps a streaming reply's text with and Copy as Markdown uses for
// a reply without its source. Its own file because both packages need it and
// neither owns the other: `CoxModel` and `CoxTranscriptText` meet only in these
// value types.

extension StyledDoc {
  /// Every block as Markdown, joined by a blank line.
  public var markdown: String { blocks.compactMap(\.markdown).joined(separator: "\n\n") }
}

extension DocBlock {
  /// This block as Markdown; `nil` for a table without rows.
  public var markdown: String? {
    func joined(_ lines: [[Span]], _ span: (Span) -> String) -> String {
      lines.map { $0.map(span).joined() }.joined(separator: "\n")
    }
    switch self {
    case .text(let kind, let lines):
      return lines.enumerated().map { Self.markdown($1, kind, first: $0 == 0) }
        .joined(separator: "\n")
    case .code(let lang, let lines): return Self.fence(lang, joined(lines, \.text))
    case .table(let rows):
      guard let head = rows.first else { return nil }
      let row = { (cells: [String]) in "| " + cells.joined(separator: " | ") + " |" }
      return ([row(head), row(head.map { _ in "---" })] + rows.dropFirst().map(row))
        .joined(separator: "\n")
    case .rule: return "---"
    }
  }

  /// A text line as Markdown: its quotes as `>`, then an item's depth indent and
  /// its number or `-`, or a heading's `#` run on its first line; a list line
  /// that goes on an item indented under it.
  static func markdown(_ line: TextLine, _ kind: TextKind, first: Bool) -> String {
    var head = String(repeating: "> ", count: Int(line.quote))
    var spans = line.spans
    if !line.marker.isEmpty {
      let number = line.marker.first?.isNumber == true
      head += String(repeating: "  ", count: Int(line.depth)) + (number ? line.marker : "-") + " "
    } else if kind == .list {
      head += String(repeating: "  ", count: Int(line.depth) + 1)
    } else if case .heading(let level) = kind {
      if first { head += String(repeating: "#", count: Int(level)) + " " }
      // Bold by its level, so its spans' bold marks would only double it.
      for index in spans.indices { spans[index].bold = false }
    }
    return head + spans.map(\.markdown).joined()
  }

  /// `body` fenced as `lang`, with a fence longer than any backtick run inside it.
  public static func fence(_ lang: String, _ body: String) -> String {
    var ticks = "```"
    while body.contains(ticks) { ticks += "`" }
    return "\(ticks)\(lang)\n\(body)\n\(ticks)"
  }
}

extension Span {
  /// The span's text with its bold, italic and strike marks; whitespace bare.
  public var markdown: String {
    guard !text.allSatisfy(\.isWhitespace) else { return text }
    var text = text
    if bold { text = "**\(text)**" }
    if italic { text = "_\(text)_" }
    if strike { text = "~~\(text)~~" }
    return text
  }
}
