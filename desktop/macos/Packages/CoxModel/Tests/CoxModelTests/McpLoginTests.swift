// MCP login through SettingsStore (T37.30.3) over the fixture client: a server shows logged
// out, Log in hands the login page to the host's `open`, and after the scripted callback the
// server shows logged in with Log out offered. No browser opens and no keychain is read.

import CoxClient
import Synchronization
import Testing

@testable import CoxModel

/// Remembers the URLs it was asked to open.
final class OpenRecorder: PlatformHost {
  let opened = Mutex<[String]>([])

  func secret(for section: String) -> String? { nil }
  func notify(_ note: HostNote) {}
  func open(_ url: String) { opened.withLock { $0.append(url) } }
}

@MainActor
@Suite struct McpLoginTests {
  let host = OpenRecorder()

  private func store() -> SettingsStore {
    let view = SettingsView(
      settings: [], userFile: "/u/config.toml",
      mcp: [
        McpServer(name: "docs", source: "config", login: .loggedOut),
        McpServer(name: "local", source: ".mcp.json", login: .stdio),
      ])
    return SettingsStore(
      client: FixtureSettingsClient(view: view, host: host), secrets: MemorySecretStore(),
      cwd: "/p")
  }

  @Test func aServerShowsLoggedOutThenLoggedInAfterTheCallback() async {
    let store = store()
    await store.load()
    #expect(
      store.logins == [
        McpLoginRow(server: "docs", detail: "Not logged in", action: .logIn),
        McpLoginRow(server: "local", detail: "Runs locally from .mcp.json; no login", action: nil),
      ])

    await store.setLogin("docs", true)
    #expect(host.opened.withLock { $0 } == [FixtureSettingsClient.loginPage])
    #expect(
      store.logins.first
        == McpLoginRow(server: "docs", detail: "Logged in, expires in 1h", action: .logOut))

    await store.setLogin("docs", false)
    #expect(store.logins.first?.action == .logIn)
    #expect(store.failure == nil)
  }

  @Test func aStdioServerHasNoLoginToRun() async {
    let store = store()
    await store.load()
    await store.setLogin("local", true)
    #expect(store.failure != nil)
    #expect(host.opened.withLock { $0 }.isEmpty)
  }
}
