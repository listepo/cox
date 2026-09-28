// A `PlatformHost` as the generated `AppHost` (DT§4.4): the app passes
// `HostBridge(MacHost())` to `LiveCoreClient`. Separate from the client
// because it runs the other way — Rust calls it — and it is the one place
// an inbox item becomes a `HostNote`.

import CoxClient
import CoxFFIBindings

public final class HostBridge: AppHost {
  private let host: any PlatformHost

  public init(_ host: any PlatformHost) { self.host = host }

  public func notify(item: InboxItem, badge: UInt32) {
    host.notify(HostNote(item, badge: badge))
  }

  public func openUrl(url: String) { host.open(url) }

  public func secret(section: String) -> String? { host.secret(for: section) }
}

extension HostNote {
  init(_ item: InboxItem, badge: UInt32) {
    let (kind, text): (Kind, String) =
      switch item.need {
      case .approval(let call, _): (.approval, call.name)
      case .question(_, let question, _): (.question, question)
      case .failed(let text): (.failed, text)
      case .taskDone(_, let label, let succeeded): (.taskDone(succeeded: succeeded), label)
      }
    self.init(session: item.session, kind: kind, text: text, badge: Int(badge))
  }
}
