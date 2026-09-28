// `.textStyle(_:)` (DS§3.2, DS§6.1): font, line height, tracking and tabular digits from a
// `FontToken`, at the user's text size. Separate so no view spells a font and every text
// scales together.

import AppKit
import SwiftUI

extension View {
  /// Sets the token's font at the user's text size; `tabularDigits` for numbers that change
  /// while you watch (always on for `.metric`).
  func textStyle(_ token: FontToken, tabularDigits: Bool = false) -> some View {
    modifier(TextStyle(token: token, tabularDigits: tabularDigits || token == .metric))
  }

  /// An SF Symbol at the token's size, weight medium, rendered hierarchical (DS§3.7), so a
  /// glyph scales with the text beside it.
  func symbolStyle(_ token: FontToken = .body) -> some View {
    textStyle(token).fontWeight(.medium).symbolRenderingMode(.hierarchical)
  }
}

private struct TextStyle: ViewModifier {
  let token: FontToken
  let tabularDigits: Bool
  @EffectiveAppearance private var appearance

  func body(content: Content) -> some View {
    let size = token.size * appearance.textScale
    let font = Font.system(size: size, weight: token.weight, design: token.design)
    content
      .font(tabularDigits ? font.monospacedDigit() : font)
      .lineSpacing(max(0, size * token.lineHeight - Self.naturalLineHeight(size, token.design)))
      .tracking(token.tracking * size)
  }

  /// The system font's own line height, which SwiftUI's `lineSpacing` adds to.
  private static func naturalLineHeight(_ size: CGFloat, _ design: Font.Design) -> CGFloat {
    let font =
      design == .monospaced
      ? NSFont.monospacedSystemFont(ofSize: size, weight: .regular)
      : NSFont.systemFont(ofSize: size)
    return font.ascender - font.descender + font.leading
  }
}
