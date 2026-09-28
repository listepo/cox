// `CodeRun` (DS§6.3 rows `DiffLineView` and `CodeBlockView`, the mockup's `.kw`, `.str`, `.num`,
// `.fn`, `.com`, `.ty`): a stretch of code text in one syntax role, as the core highlighted it.
// Separate so a diff line and a code block colour the same roles the same way; the app maps
// Rust `StyledDoc` spans to these runs and a view never highlights on its own.

import SwiftUI

/// Text and the DS§3.1 `syntax` role that colours it; `plain` takes the surrounding colour.
public struct CodeRun: Equatable, Sendable {
  public enum Role: CaseIterable, Sendable {
    case plain, keyword, string, number, function, comment, type
  }

  public var text: String
  public var role: Role

  public init(_ text: String, _ role: Role = .plain) {
    self.text = text
    self.role = role
  }

  /// Lines of runs as one attributed string, so one `Text` draws a whole block.
  static func attributed(_ lines: [[CodeRun]]) -> AttributedString {
    var joined = AttributedString()
    for (index, line) in lines.enumerated() {
      if index > 0 { joined.append(AttributedString("\n")) }
      for run in line { joined.append(run.attributed) }
    }
    return joined
  }

  private var attributed: AttributedString {
    var text = AttributedString(self.text)
    text.foregroundColor = role.colour
    return text
  }
}

extension CodeRun.Role {
  /// `nil` for plain text, which keeps the foreground the view sets.
  var colour: Color? {
    switch self {
    case .plain: nil
    case .keyword: Color(.syntaxKeyword)
    case .string: Color(.syntaxString)
    case .number: Color(.syntaxNumber)
    case .function: Color(.syntaxFunction)
    case .comment: Color(.syntaxComment)
    case .type: Color(.syntaxType)
    }
  }
}
