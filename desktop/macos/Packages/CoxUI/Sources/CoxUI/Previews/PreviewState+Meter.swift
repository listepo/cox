// `PreviewState` fixtures for the progress and token-meter atoms (T37.20.1–T37.20.2): the values
// their `#Preview`s and snapshot tests share. Separate from `PreviewState.swift` so the atoms
// built in parallel add their fixtures without editing one file.

import SwiftUI

extension PreviewState {
  /// Empty, the mockup's cost-capsule context share, and full.
  static let fractions = [0, 0.38, 1]
}
