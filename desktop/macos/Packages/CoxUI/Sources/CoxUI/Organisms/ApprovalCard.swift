// `ApprovalCard` (DS§6.4 row `ApprovalCard`, the mockup's `.appr`, DT§5.2 Approval): a tool call
// that waits on the person — what it will run, why they are asked, which subagent asks — with
// Allow, Allow for session and Deny; once decided, the one line that says how. Separate so the
// transcript card and the pinned bar above the composer draw an approval alike. `DecisionFrame`
// here is shared with `QuestionCard`, the other card that waits on the person.

import SwiftUI

/// Pending: a header with a warning symbol and the risk, the command in a code well, the
/// reasons in `text.secondary`, then the buttons on `fill.primary` under a hairline — a readable
/// face at e1 with an orange edge. Decided: a `NoticeRow`, "Allowed by you · for session".
public struct ApprovalCard: View {
  /// What the card shows; every string comes formatted from the transcript's mapping.
  public struct Content: Equatable, Sendable {
    /// `Run this command?`
    public var title: String
    /// The command or call, drawn monospaced in the well.
    public var command: String
    /// Why the person is asked, `matches ask rule Bash(git push:*)`.
    public var reason: String
    /// The subagent that asks; `nil` for the main agent.
    public var source: String?
    public var risk: ToolHeader.Risk?
    /// Set once decided: the card shrinks to this line.
    public var outcome: Outcome?

    public init(
      title: String, command: String, reason: String, source: String? = nil,
      risk: ToolHeader.Risk? = nil, outcome: Outcome? = nil
    ) {
      (self.title, self.command, self.reason) = (title, command, reason)
      (self.source, self.risk, self.outcome) = (source, risk, outcome)
    }
  }

  /// How the call was decided, `Allowed by you · for session`.
  public struct Outcome: Equatable, Sendable {
    public var text: String
    public var isAllowed: Bool

    public init(text: String, isAllowed: Bool) {
      self.text = text
      self.isAllowed = isAllowed
    }
  }

  /// The buttons. Edit needs the call's input, which the approval block does not carry yet.
  public enum Action: CaseIterable, Sendable {
    case allow, allowForSession, deny
  }

  let content: Content
  let act: (Action) -> Void

  public init(_ content: Content, act: @escaping (Action) -> Void) {
    self.content = content
    self.act = act
  }

  public var body: some View {
    if let outcome = content.outcome {
      NoticeRow(outcome.text, symbol: outcome.isAllowed ? "checkmark.circle" : "xmark.circle")
    } else {
      DecisionFrame(edge: Color(.statusWarning)) {
        DecisionTitle(
          content.title, symbol: "exclamationmark.triangle", tint: Color(.statusWarning)
        ) {
          if let risk = content.risk { RiskChip(risk.text, level: risk.level) }
        }
        Text(content.command)
          .textStyle(.monoCode)
          .foregroundStyle(Color(.textPrimary))
          .textSelection(.enabled)
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(.horizontal, Space.l)
          .padding(.vertical, Space.m)
          .background {
            RoundedRectangle(cornerRadius: Radius.m, style: .continuous).fill(Color(.surfaceCode))
          }
          .hairline(in: RoundedRectangle(cornerRadius: Radius.m, style: .continuous))
        VStack(alignment: .leading, spacing: Space.xs) {
          DecisionReason(label: "Why you are asked:", text: content.reason)
          if let source = content.source { DecisionReason(label: "Asked by:", text: source) }
        }
      } actions: {
        Button("Allow") { act(.allow) }.buttonStyle(CoxButtonStyle(.primary, size: .small))
        Button("Allow for session") { act(.allowForSession) }
          .buttonStyle(CoxButtonStyle(.secondary, size: .small))
        Button("Deny") { act(.deny) }.buttonStyle(CoxButtonStyle(.danger, size: .small))
        Spacer(minLength: 0)
      }
    }
  }
}

/// The mockup's `.appr` shell both waiting cards share: the body, then the action row on
/// `fill.primary` under a hairline, on a readable face at e1 with a coloured leading edge.
struct DecisionFrame<Body: View, Actions: View>: View {
  let edge: Color
  @ViewBuilder let content: Body
  @ViewBuilder let actions: Actions

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.xxl, style: .continuous)
    VStack(alignment: .leading, spacing: 0) {
      VStack(alignment: .leading, spacing: Space.m) { content }
        .padding(.top, Space.l)
        .padding(.bottom, Space.ml)
        .padding(.leading, Space.xxl)
        .padding(.trailing, Space.xl)
      HStack(spacing: Space.m) { actions }
        .padding(.vertical, Space.ml)
        .padding(.leading, Space.xxl)
        .padding(.trailing, Space.xl)
        .background(Color(.fillPrimary))
        .hairline(.top)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .background { Color.clear.glassPane(shape, role: .readable) }
    // The mockup's 4 px edge: the nearest step, `space.xs`.
    .overlay(alignment: .leading) { edge.frame(width: Space.xs) }
    .clipShape(shape)
    .hairline(in: shape)
    .elevation(.e1, cornerRadius: Radius.xxl)
  }
}

/// A waiting card's header: its symbol in `tint`, the title, then any trailing chips.
struct DecisionTitle<Trailing: View>: View {
  let title: String
  let symbol: String
  let tint: Color
  @ViewBuilder let trailing: Trailing

  init(
    _ title: String, symbol: String, tint: Color,
    @ViewBuilder trailing: () -> Trailing = { EmptyView() }
  ) {
    (self.title, self.symbol, self.tint) = (title, symbol, tint)
    self.trailing = trailing()
  }

  var body: some View {
    HStack(spacing: Space.m) {
      Image(systemName: symbol).symbolStyle(.titleWindow).foregroundStyle(tint)
        .accessibilityHidden(true)
      Text(title).textStyle(.titleWindow).foregroundStyle(Color(.textPrimary))
      trailing
      Spacer(minLength: 0)
    }
  }
}

/// One reason line, the mockup's `.why`: the label in `text.primary`, the rest secondary.
private struct DecisionReason: View {
  let label: String
  let text: String

  var body: some View {
    let rest = Text(text).foregroundStyle(Color(.textSecondary))
    Text("\(Text(label).fontWeight(.medium).foregroundStyle(Color(.textPrimary))) \(rest)")
      .textStyle(.caption)
      .fixedSize(horizontal: false, vertical: true)
  }
}

#Preview("pending") { PreviewMatrix { ApprovalCardSample(PreviewState.approvalPending) } }
#Preview("subagent, risky") {
  PreviewMatrix { ApprovalCardSample(PreviewState.approvalRisky) }
}
#Preview("allowed") { PreviewMatrix { ApprovalCardSample(PreviewState.approvalAllowed) } }
#Preview("denied") { PreviewMatrix { ApprovalCardSample(PreviewState.approvalDenied) } }
