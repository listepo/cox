// A `PlatformHost` as the generated `AppHost` (DT§4.4): the app passes
// `HostBridge(MacHost())` to `LiveCoreClient`. Separate from the client
// because it runs the other way — Rust calls it — and it is the one place
// cox-ffi's inbox item becomes CoxClient's, which `HostNote` is made from.

import CoxClient
import CoxFFIBindings

public final class HostBridge: AppHost {
  private let host: any PlatformHost

  public init(_ host: any PlatformHost) { self.host = host }

  public func notify(item: CoxFFIBindings.InboxItem, badge: UInt32) {
    host.notify(HostNote(CoxClient.InboxItem(item), badge: Int(badge)))
  }

  public func badge(badge: UInt32) { host.badge(Int(badge)) }

  public func openUrl(url: String) { host.open(url) }

  public func secret(section: String) -> String? { host.secret(for: section) }
}

extension CoxClient.InboxItem {
  init(_ item: CoxFFIBindings.InboxItem) {
    let need: CoxClient.Need =
      switch item.need {
      case .approval(let call, let why):
        .approval(call: call.id, tool: call.name, subject: call.subject, why: .init(why))
      case .question(let call, let question, let options):
        .question(call: call, question: question, options: options)
      case .failed(let text): .failed(text: text)
      case .taskDone(let task, let label, let succeeded):
        .taskDone(task: task, label: label, succeeded: succeeded)
      }
    self.init(
      session: item.session,
      source: item.source.map { .init(session: $0.session, agent: $0.agent, preset: $0.preset) },
      need: need, expired: item.expired, seq: item.seq)
  }
}
