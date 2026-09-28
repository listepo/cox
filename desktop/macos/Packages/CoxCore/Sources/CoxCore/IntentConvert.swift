// CoxClient's `Intent` into cox-ffi's, the one conversion that flows from Swift to Rust, apart
// from `Convert.swift`'s timeline so each file stays one concern. Case for case; exhaustive, so a
// variant added in Rust fails this build.

import CoxClient
import CoxFFIBindings

extension CoxFFIBindings.Intent {
  init(_ value: CoxClient.Intent) {
    switch value {
    case .send(let text, let attachments):
      self = .send(text: text, attachments: attachments.map { .init($0) })
    case .approve(let call, let decision): self = .approve(call: call, decision: .init(decision))
    case .answer(let question, let text): self = .answer(question: question, text: text)
    case .interrupt: self = .interrupt
    case .queue(let text, let attachments):
      self = .queue(text: text, attachments: attachments.map { .init($0) })
    case .compact(let focus): self = .compact(focus: focus)
    case .setMode(let mode):
      switch mode {
      case .default: self = .setMode(mode: .default)
      case .plan: self = .setMode(mode: .plan)
      case .auto: self = .setMode(mode: .auto)
      case .bypass: self = .setMode(mode: .bypass)
      }
    case .switchModel(let tier, let model): self = .switchModel(tier: .init(tier), model: model)
    case .setEffort(let effort):
      switch effort {
      case nil: self = .setEffort(effort: nil)
      case .low: self = .setEffort(effort: .low)
      case .medium: self = .setEffort(effort: .medium)
      case .high: self = .setEffort(effort: .high)
      case .xhigh: self = .setEffort(effort: .xhigh)
      }
    case .rewind(let toTurn, let code, let conversation):
      self = .rewind(toTurn: toTurn, code: code, conversation: conversation)
    case .redo: self = .redo
    case .revertFile(let path, let toTurn): self = .revertFile(path: path, toTurn: toTurn)
    case .fork(let turn): self = .fork(turn: turn)
    case .handoff(let objective): self = .handoff(objective: objective)
    case .background(let call): self = .background(call: call)
    case .shell(let command, let share): self = .shell(command: command, share: share)
    case .command(let line): self = .command(line: line)
    }
  }
}

extension CoxFFIBindings.Attachment {
  init(_ value: CoxClient.Attachment) {
    self.init(name: value.name, mediaType: value.mediaType, dataB64: value.dataB64)
  }
}
