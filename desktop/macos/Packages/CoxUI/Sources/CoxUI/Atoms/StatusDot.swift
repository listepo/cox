// `StatusDot` (DS§6.2 row `StatusDot`, the mockup's `.dot .d-*`): a session's state as a
// `Size.statusDot` dot. Separate so the sidebar, the session header and the footer show a
// state in one colour and shape.

import SwiftUI

/// Running and waiting glow with a soft halo; idle is a hollow ring; error is a plain red dot.
struct StatusDot: View {
  enum Status: CaseIterable, Sendable {
    case running, waiting, idle, error
  }

  let status: Status

  /// The mockup's halo, `0 0 0 3px` of the soft status colour.
  private static let haloWidth: CGFloat = 3
  /// The mockup's idle ring, `border: 1.5px`.
  private static let ringWidth: CGFloat = 1.5

  init(_ status: Status) {
    self.status = status
  }

  var body: some View {
    Group {
      if status == .idle {
        Circle().strokeBorder(Color(.textTertiary), lineWidth: Self.ringWidth)
      } else {
        Circle().fill(status.fill)
      }
    }
    .frame(width: Size.statusDot, height: Size.statusDot)
    .background {
      if let halo = status.halo {
        Circle().fill(halo).padding(-Self.haloWidth)
      }
    }
    .accessibilityElement()
    .accessibilityLabel(status.label)
  }
}

extension StatusDot.Status {
  var fill: Color {
    switch self {
    case .running: Color(.statusSuccess)
    case .waiting: Color(.statusWarning)
    case .idle: .clear
    case .error: Color(.statusDanger)
    }
  }

  /// Only a live state glows: it is the one to look at.
  var halo: Color? {
    switch self {
    case .running: Color(.statusSuccessSoft)
    case .waiting: Color(.statusWarningSoft)
    case .idle, .error: nil
    }
  }

  var label: String {
    switch self {
    case .running: "Running"
    case .waiting: "Waiting for you"
    case .idle: "Idle"
    case .error: "Error"
    }
  }
}

#Preview("running") { PreviewMatrix { StatusDot(.running) } }
#Preview("waiting") { PreviewMatrix { StatusDot(.waiting) } }
#Preview("idle") { PreviewMatrix { StatusDot(.idle) } }
#Preview("error") { PreviewMatrix { StatusDot(.error) } }
