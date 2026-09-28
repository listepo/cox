// A reply's `StyledDoc` back as Markdown (T37.42, T37.42.1), its list
// bullets, quote rails and heading `#` run — text in the doc — back as
// Markdown's own (T37.23.8). A streamed `docTail` carries doc blocks, not
// source, so this is the one rendering the session store keeps a streaming
// reply's text with and Copy as Markdown uses for a reply without its
// source. Its own file because both packages need it and neither owns the
// other: `CoxModel` and `CoxTranscriptText` meet only in these value types.

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
    case .text(.heading(let level), var lines):
      if let first = lines.first, Self.marker(first)?.allSatisfy({ "# ".contains($0) }) == true {
        lines[0].removeFirst()
      }
      // Bold by its level, so its spans' bold marks would only double it.
      return String(repeating: "#", count: Int(level)) + " "
        + joined(lines) { span in
          var plain = span
          plain.bold = false
          return plain.markdown
        }
    case .text(let kind, let lines) where kind == .list || kind == .quote:
      return lines.map { Self.markdown($0, quote: kind == .quote) }.joined(separator: "\n")
    case .text(_, let lines): return joined(lines, \.markdown)
    case .code(let lang, let lines): return Self.fence(lang, joined(lines, \.text))
    case .table(let rows):
      guard let head = rows.first else { return nil }
      let row = { (cells: [String]) in "| " + cells.joined(separator: " | ") + " |" }
      return ([row(head), row(head.map { _ in "---" })] + rows.dropFirst().map(row))
        .joined(separator: "\n")
    case .rule: return "---"
    }
  }

  /// A line's leading marker as `cox-render` lays it out — a heading's `#` run,
  /// a list item's depth indent and bullet or number, a quote's rails — which
  /// the doc carries as text (T37.23.8); `nil` when the line starts with its
  /// content. A marker is the line's own first span, ends in a space and holds
  /// no letter.
  public static func marker(_ line: some Collection<Span>) -> String? {
    guard line.count > 1, let text = line.first?.text, text.hasSuffix(" "),
      text.contains(where: { !$0.isWhitespace }), !text.contains(where: \.isLetter)
    else { return nil }
    return text
  }

  /// A list or quote line as Markdown: rails as `>`, a bullet as `-`, a number
  /// as it is, each item's depth indent kept.
  static func markdown(_ line: [Span], quote: Bool) -> String {
    var rest = line[...]
    var head = ""
    if quote, let rails = marker(rest) {
      head = String(repeating: "> ", count: rails.count { !$0.isWhitespace })
      rest = rest.dropFirst()
    }
    if let item = marker(rest) {
      let glyph = item.trimmingCharacters(in: .whitespaces)
      head += String(item.prefix { $0 == " " }) + (glyph.hasSuffix(".") ? glyph : "-") + " "
      rest = rest.dropFirst()
    }
    return head + rest.map(\.markdown).joined()
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
