// A reply's `StyledDoc` back as Markdown (T37.42, T37.42.1). A streamed
// `docTail` carries doc blocks, not source, so this is the one rendering the
// session store keeps a streaming reply's text with and Copy as Markdown uses
// for a reply without its source. Its own file because both packages need it
// and neither owns the other: `CoxModel` and `CoxTranscriptText` meet only
// in these value types.

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
    case .text(.heading(let level), let lines):
      return String(repeating: "#", count: Int(level)) + " " + joined(lines, \.markdown)
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
