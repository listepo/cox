// `Badge` (DS§6.2 row `Badge`, the mockup's `.badge .b-*`): a short tag in a soft tint of its
// meaning — where a setting comes from, an agent's model, a warning. Separate so every tag has
// one shape and one colour per kind; `RiskChip` is a badge too.

import SwiftUI

/// Tinted text on a tinted, flat rounded face.
struct Badge: View {
  enum Kind: CaseIterable, Sendable {
    case neutral, user, project, env, `default`, warning, danger
  }

  let text: String
  let kind: Kind

  /// The mockup's `border-radius: 5px`, between `Radius.xs` and `Radius.s`.
  private static let cornerRadius: CGFloat = 5
  /// The mockup's `padding: 1px 6px`: the vertical step is below `Space.xxs`.
  private static let verticalPadding: CGFloat = 1

  init(_ text: String, kind: Kind = .neutral) {
    self.text = text
    self.kind = kind
  }

  var body: some View {
    Text(text)
      .textStyle(.micro)
      .lineLimit(1)
      .foregroundStyle(kind.foreground)
      .padding(.horizontal, Space.s)
      .padding(.vertical, Self.verticalPadding)
      .background(
        kind.background,
        in: RoundedRectangle(cornerRadius: Self.cornerRadius, style: .continuous))
  }
}

extension Badge.Kind {
  /// The mockup's `.b-project` fill, `rgba(142,68,216,.13)`: the tint over its colour.
  private static let projectTint = 0.13

  var foreground: Color {
    switch self {
    case .neutral, .default: Color(.textSecondary)
    case .user: Color(.accent)
    // No project role exists; the plan colour stands in for the mockup's purple.
    case .project: Color(.statusPlan)
    case .env: Color(.statusSuccess)
    case .warning: Color(.statusWarning)
    case .danger: Color(.statusDanger)
    }
  }

  var background: Color {
    switch self {
    case .neutral, .default: Color(.fillSecondary)
    case .user: Color(.accentSoft)
    case .project: Color(.statusPlan).opacity(Self.projectTint)
    case .env: Color(.statusSuccessSoft)
    case .warning: Color(.statusWarningSoft)
    case .danger: Color(.statusDangerSoft)
    }
  }
}

#Preview("neutral") { PreviewMatrix { Badge(PreviewState.badge(.neutral), kind: .neutral) } }
#Preview("user") { PreviewMatrix { Badge(PreviewState.badge(.user), kind: .user) } }
#Preview("project") { PreviewMatrix { Badge(PreviewState.badge(.project), kind: .project) } }
#Preview("env") { PreviewMatrix { Badge(PreviewState.badge(.env), kind: .env) } }
#Preview("default") { PreviewMatrix { Badge(PreviewState.badge(.default), kind: .default) } }
#Preview("warning") { PreviewMatrix { Badge(PreviewState.badge(.warning), kind: .warning) } }
#Preview("danger") { PreviewMatrix { Badge(PreviewState.badge(.danger), kind: .danger) } }
