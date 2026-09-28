// Settings through the real Rust core (T37.30): a scratch home, an edit that
// lands in its `config.toml` and comes back with the `user` layer. The host
// has no secrets, so nothing reaches the Keychain (A49).

import CoxClient
import CoxFFIBindings
import Foundation
import Testing

@testable import CoxCore

final class SilentHost: AppHost {
  func notify(item: InboxItem, badge: UInt32) {}
  func openUrl(url: String) {}
  func secret(section: String) -> String? { nil }
}

@Test func anEditRoundTripsThroughRustIntoTheUserLayer() async throws {
  let home = FileManager.default.temporaryDirectory.appending(path: "cox-settings-\(UUID())")
  defer { try? FileManager.default.removeItem(at: home) }
  let client = try LiveCoreClient(home: home.path(), host: SilentHost())
  let key = "desktop.appearance.material"

  let before = try await client.settings(cwd: home.path())
  #expect(before.settings.first { $0.key == key }?.layer == .default)

  let after = try await client.setSetting(cwd: home.path(), key: key, json: "\"solid\"")
  let row = try #require(after.settings.first { $0.key == key })
  #expect(row.layer == .user)
  #expect(row.value == "\"solid\"")
  #expect(row.kind == .choice(options: ["frosted", "glossy", "solid"]))
  #expect(after.userFile == home.appending(path: "config.toml").path())
}

@Test func aValueTheLoaderRejectsIsASettingsError() async throws {
  let home = FileManager.default.temporaryDirectory.appending(path: "cox-settings-\(UUID())")
  defer { try? FileManager.default.removeItem(at: home) }
  let client = try LiveCoreClient(home: home.path(), host: SilentHost())
  await #expect {
    try await client.setSetting(cwd: home.path(), key: "desktop.appearance.opacity", json: "1.5")
  } throws: { error in
    if case AppError.Settings = error { true } else { false }
  }
}
