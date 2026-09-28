// `RiskChip` (DS§6.2 row `RiskChip`, the mockup's `.risk`): what a tool call may do — "network
// · writes remote" — tinted by how risky the classifier found it. Separate so a risk reads the
// same on a tool row and an approval card; it draws as a `Badge`, the mockup's `.risk` and
// `.badge` being the same shape.

import SwiftUI

/// The reason text as a badge: low is quiet, medium warns, high is danger.
public struct RiskChip: View {
  public enum Level: CaseIterable, Sendable {
    case low, medium, high
  }

  let text: String
  let level: Level

  init(_ text: String, level: Level) {
    self.text = text
    self.level = level
  }

  public var body: some View {
    Badge(text, kind: level.badge)
      .accessibilityElement(children: .ignore)
      .accessibilityLabel("\(level.label): \(text)")
  }
}

extension RiskChip.Level {
  var badge: Badge.Kind {
    switch self {
    case .low: .neutral
    case .medium: .warning
    case .high: .danger
    }
  }

  var label: String {
    switch self {
    case .low: "Low risk"
    case .medium: "Medium risk"
    case .high: "High risk"
    }
  }
}

#Preview("low") { PreviewMatrix { RiskChip(PreviewState.risk, level: .low) } }
#Preview("medium") { PreviewMatrix { RiskChip(PreviewState.risk, level: .medium) } }
#Preview("high") { PreviewMatrix { RiskChip(PreviewState.risk, level: .high) } }
