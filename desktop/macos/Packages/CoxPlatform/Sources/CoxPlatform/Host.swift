// The app's host (DT§4.4, T37.30.2): `secret` reads `KeychainSecretStore`,
// `notify` posts through `UNUserNotificationCenter`, `open` goes through
// `NSWorkspace`. Separate from the Keychain store because this is the
// AppKit side; the store stays usable without it. Only `secret` and the URL
// check are tested: the notification centre needs an app bundle, and a test
// must never post a notification or open a URL.

import AppKit
import CoxClient
import Foundation
import UserNotifications

public struct MacHost: PlatformHost {
  private let secrets: any SecretStore

  public init(secrets: any SecretStore = KeychainSecretStore()) { self.secrets = secrets }

  /// A Keychain error reads as no key: Rust then reports the key missing and
  /// names the setting, which is the remedy the person can act on.
  public func secret(for section: String) -> String? {
    try? secrets.secret(for: section)
  }

  public func notify(_ note: HostNote) {
    Task {
      let center = UNUserNotificationCenter.current()
      let allowed = try? await center.requestAuthorization(options: [.alert, .badge, .sound])
      guard allowed == true else { return }
      let content = UNMutableNotificationContent()
      content.title = Self.title(note.kind)
      content.body = note.text
      content.threadIdentifier = note.session
      let request = UNNotificationRequest(
        identifier: UUID().uuidString, content: content, trigger: nil)
      try? await center.add(request)
      try? await center.setBadgeCount(note.badge)
    }
  }

  public func open(_ url: String) {
    guard let url = Self.openable(url) else { return }
    Task { @MainActor in _ = NSWorkspace.shared.open(url) }
  }

  static func title(_ kind: HostNote.Kind) -> String {
    switch kind {
    case .approval: "Approval needed"
    case .question: "Question"
    case .failed: "Turn failed"
    case .taskDone(let succeeded): succeeded ? "Task done" : "Task failed"
    }
  }

  /// Web links only: the URL comes from an MCP server or the model, and a
  /// `file:` or another app's scheme would launch something, not show a page.
  static func openable(_ text: String) -> URL? {
    guard let url = URL(string: text), let scheme = url.scheme?.lowercased(),
      scheme == "https" || scheme == "http", url.host() != nil
    else { return nil }
    return url
  }
}
