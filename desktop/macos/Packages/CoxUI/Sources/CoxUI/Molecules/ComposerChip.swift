// `ComposerChip` (DS§6.3 row `ComposerChip`, the mockup's `.chip` and `.chip.blue`): one thing
// the next message carries besides its text — an @-mentioned file, an attachment, a slash
// command or mode, shell mode, the prompts queued behind the running turn — with a way to take
// it back out. Separate so the composer shows everything
// it will send as the same small lifted pill.

import SwiftUI

/// The kind's symbol, the label in `font.caption`, an optional `KeyCap`, and an `xmark` that
/// removes the chip, on a readable capsule face lifted to e1 (DS§3.4). Mentions and commands
/// change what the model sees, and queued prompts wait on it, so they are tinted `accent`.
struct ComposerChip: View {
  enum Kind: CaseIterable, Sendable {
    case mention, attachment, command, shell, queued
  }

  let label: String
  let kind: Kind
  /// The shortcut that toggles the chip, `⇧⇥`, or `nil`.
  let shortcut: String?
  /// Takes the chip out of the message; `nil` for a chip that cannot be removed.
  let onRemove: (() -> Void)?

  init(
    _ label: String, kind: Kind, shortcut: String? = nil, onRemove: (() -> Void)? = nil
  ) {
    self.label = label
    self.kind = kind
    self.shortcut = shortcut
    self.onRemove = onRemove
  }

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.capsule, style: .continuous)
    // The mockup's 5 px gap and 26 px height take the nearest steps, `Space.xs` and the small
    // button height.
    HStack(spacing: Space.xs) {
      Image(systemName: kind.symbol).symbolStyle(.caption)
      Text(label)
        .textStyle(.caption)
        .lineLimit(1)
        .truncationMode(.middle)
      if let shortcut { KeyCap(shortcut) }
      if let onRemove { RemoveButton(label: label, action: onRemove) }
    }
    .foregroundStyle(kind.foreground)
    .padding(.horizontal, Space.ml)
    .frame(height: Size.buttonHeightSmall)
    // Face behind the label, so the glass sweep never washes out the text (DS§8).
    .background {
      shape.fill(kind.tint).glassPane(shape, surface: Color(.surfaceCapsule), role: .readable)
    }
    .hairline(in: shape)
    .elevation(.e1, cornerRadius: Radius.capsule)
    .accessibilityElement(children: .contain)
  }
}

/// The chip's `xmark`: a bare glyph, named for VoiceOver and the tooltip after what it removes.
private struct RemoveButton: View {
  let label: String
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      Image(systemName: "xmark")
        .symbolStyle(.micro)
        .foregroundStyle(Color(.textTertiary))
        .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .help("Remove \(label)")
    .accessibilityLabel("Remove \(label)")
  }
}

extension ComposerChip.Kind {
  var symbol: String {
    switch self {
    case .mention: "at"
    case .attachment: "paperclip"
    case .command: "bolt"
    case .shell: "terminal"
    case .queued: "clock"
    }
  }

  var foreground: Color {
    switch self {
    case .mention, .command, .queued: Color(.accent)
    case .attachment, .shell: Color(.textSecondary)
    }
  }

  /// The mockup's `.chip.blue` tint over the capsule face.
  var tint: Color {
    switch self {
    case .mention, .command, .queued: Color(.accentSoft)
    case .attachment, .shell: .clear
    }
  }
}

#Preview("mention") { PreviewMatrix { ComposerChipSample(kind: .mention) } }
#Preview("attachment") { PreviewMatrix { ComposerChipSample(kind: .attachment) } }
#Preview("command") { PreviewMatrix { ComposerChipSample(kind: .command) } }
#Preview("shell") { PreviewMatrix { ComposerChipSample(kind: .shell) } }
#Preview("queued") { PreviewMatrix { ComposerChipSample(kind: .queued) } }
#Preview("shortcut") { PreviewMatrix { ComposerChipSample.shortcut } }
