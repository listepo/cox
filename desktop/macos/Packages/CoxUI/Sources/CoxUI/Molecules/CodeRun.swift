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
  /// A word the core's word diff found changed in a replaced line pair (T37.23.11).
  public var isChanged: Bool

  public init(_ text: String, _ role: Role = .plain, isChanged: Bool = false) {
    self.text = text
    self.role = role
    self.isChanged = isChanged
  }

  /// Lines of runs as one attributed string, so one `Text` draws a whole block; a changed run
  /// sits on `mark`.
  static func attributed(_ lines: [[CodeRun]], mark: Color? = nil) -> AttributedString {
    var joined = AttributedString()
    for (index, line) in lines.enumerated() {
      if index > 0 { joined.append(AttributedString("\n")) }
      for run in line { joined.append(run.attributed(mark: mark)) }
    }
    return joined
  }

  private func attributed(mark: Color?) -> AttributedString {
    var text = AttributedString(self.text)
    text.foregroundColor = role.colour
    if isChanged { text.backgroundColor = mark }
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
