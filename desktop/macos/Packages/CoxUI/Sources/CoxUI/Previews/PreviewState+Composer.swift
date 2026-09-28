// `PreviewState` fixtures for the composer's molecules (T37.21.7): the chips of a message
// about to be sent, as mockup screens 1 and 6 show them. Separate from `PreviewState.swift` so
// molecules built in parallel add their fixtures without editing one file.

import SwiftUI

extension PreviewState {
  /// A chip's label per kind.
  static func chip(_ kind: ComposerChip.Kind) -> String {
    switch kind {
    case .mention: "crates/cox-provider/src/retry.rs"
    case .attachment: fileName
    case .command: "/review"
    }
  }

  static let modeChip = "Plan"
  static let modeShortcut = "⇧⇥"
}

/// A removable chip of `kind`.
struct ComposerChipSample: View {
  let kind: ComposerChip.Kind

  var body: some View {
    ComposerChip(PreviewState.chip(kind), kind: kind) {}
  }

  /// The mode chip: a command with its shortcut that stays in the composer.
  static var shortcut: some View {
    ComposerChip(PreviewState.modeChip, kind: .command, shortcut: PreviewState.modeShortcut)
  }
}
