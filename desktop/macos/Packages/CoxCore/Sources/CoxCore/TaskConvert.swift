// A task's kind and what opening it shows, from cox-ffi into CoxClient's (T37.29.6), apart from
// `Convert.swift`'s timeline so each file stays one concern. Case for case; nothing is decided
// here.

import CoxClient
import CoxFFIBindings

extension CoxClient.TaskKind {
  init(_ value: CoxFFIBindings.TaskKind) {
    switch value {
    case .agent: self = .agent
    case .shell: self = .shell
    }
  }
}

extension CoxClient.TaskTarget {
  init(_ value: CoxFFIBindings.TaskTarget) {
    switch value {
    case .transcript(let session): self = .transcript(session: session)
    case .output(let archive): self = .output(archive: archive)
    }
  }
}
