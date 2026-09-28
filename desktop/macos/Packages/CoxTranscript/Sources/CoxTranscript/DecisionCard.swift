// The card an approval or question block shows (T37.27, DT§5.2): CoxUI's `ApprovalCard` or
// `QuestionCard`, filled from the block, with what the person picks sent as an `Intent`. Separate
// from `TranscriptCard` so the one place that turns a waiting block into a card and a choice
// into an intent is its own file; like `TranscriptCard`, it maps enums to words and lays out.

import CoxClient
import CoxModel
import CoxUI
import SwiftUI

/// An approval or question block as its card; an Allow, Deny or answer goes to `send`. Nothing
/// for any other block.
public struct DecisionCard: View {
  let block: Block
  let send: @MainActor (Intent) -> Void

  public init(block: Block, send: @escaping @MainActor (Intent) -> Void) {
    self.block = block
    self.send = send
  }

  public var body: some View {
    switch block.kind {
    case .approval(let call, _, _, _, _, _, _):
      if let content = ApprovalCard.Content(block) {
        ApprovalCard(content) { send(.approve(call: call, decision: $0.decision)) }
      }
    case .question(let call, _, _, _):
      if let content = QuestionCard.Content(block) {
        QuestionCard(content) { send(.answer(question: call, text: $0)) }
      }
    default:
      EmptyView()
    }
  }
}

extension TranscriptView where Approval == DecisionCard {
  /// The transcript with its approvals and questions as cards; the person's choices go to
  /// `send`, which the app forwards to the session and reports a failure from.
  public init(
    store: SessionStore, crossBlockSelection: Bool = true,
    send: @escaping @MainActor (Intent) -> Void
  ) {
    self.init(store: store, crossBlockSelection: crossBlockSelection) {
      DecisionCard(block: $0, send: send)
    }
  }
}

extension ApprovalCard.Action {
  /// The TUI's deny reason (`cox-tui` `modal.rs`), so the model reads the same words.
  var decision: Decision {
    switch self {
    case .allow: .allow
    case .allowForSession: .allowForSession
    case .deny: .deny(reason: "denied by user")
    }
  }
}

extension ApprovalCard.Content {
  /// The card of an approval block; `nil` for any other block.
  init?(_ block: Block) {
    guard
      case .approval(_, let tool, let summary, let why, let source, let decision, let decidedBy) =
        block.kind
    else { return nil }
    let risk: ToolHeader.Risk? = if case .risk(let risk) = why { risk.chip } else { nil }
    self.init(
      title: tool == "bash" ? "Run this command?" : "Allow \(tool)?", command: summary,
      reason: why.text, source: source?.label, risk: risk,
      outcome: decision.map { .init(text: $0.outcome(by: decidedBy), isAllowed: $0.allows) })
  }
}

extension QuestionCard.Content {
  /// The card of a question block; `nil` for any other block.
  init?(_ block: Block) {
    guard case .question(_, let question, let options, let answer) = block.kind else { return nil }
    self.init(question: question, options: options, answer: answer)
  }
}

extension Why {
  /// DT§5.2's "why" line.
  var text: String {
    switch self {
    case .ruleAsk(let rule): "matches ask rule \(rule)"
    case .risk(let risk): "risk: \(risk.words)"
    case .sandboxDenied(let detail): "the sandbox denied it: \(detail)"
    case .policy(let policy): "approval policy \(policy.rawValue)"
    }
  }
}

extension Risk {
  var words: String {
    switch self {
    case .readOnly: "reads only"
    case .write: "writes files"
    case .exec: "runs a command"
    case .destructive: "destructive"
    }
  }
}

extension Source {
  /// The subagent's name, else its preset.
  var label: String? { agent ?? preset }
}

extension Decision {
  var allows: Bool { if case .deny = self { false } else { true } }

  /// The line a decided card shrinks to, `Allowed by you · for session` (DT§5.2).
  func outcome(by decider: DecidedBy?) -> String {
    let verb =
      switch self {
      case .allow, .allowForSession: "Allowed"
      case .deny: "Denied"
      case .edit: "Edited and allowed"
      }
    let who = decider.map { " \($0.words)" } ?? ""
    return verb + who + (self == .allowForSession ? " · for session" : "")
  }
}

extension DecidedBy {
  var words: String {
    switch self {
    case .user: "by you"
    case .rule: "by a rule"
    case .session: "by a session grant"
    case .policy: "by the approval policy"
    case .hook: "by a hook"
    }
  }
}
