// The app target's entry (DT§4.6, DT§7, A106): `@main`, its scenes and the launch-wide state
// they share. Thin by design — every view and store lives in the local packages; this target
// picks the core at launch, hosts the windows and joins stores to screens. The session window
// hides its title bar (DS§4); a session pops out into its own window or a native tab (T51.11);
// Settings opens from the app menu (⌘,); the menu-bar extra shows what needs you (T51.14);
// two recorded global hotkeys open that menu and a new session (T51.15); session titles are
// in Spotlight, and a result opens its session (T51.16); App Intents ask cox in a project and
// open a session (T51.17).

import AppKit
import CoxClient
import CoxCore
import CoxModel
import CoxPlatform
import SwiftUI
import UserNotifications

@main
struct CoxApp: App {
  @State private var model = AppModel(launch: LaunchCore.pick())

  /// The main window's scene id: the menu bar's New session and Open cox open one.
  static let mainWindow = "main"

  /// The setting decides; removing the extra from the menu bar lasts until relaunch, and
  /// Settings is where it is turned off for good.
  private var showsMenuBar: Binding<Bool> {
    Binding(get: { model.settings?.showsMenuBar ?? true }, set: { _ in })
  }

  var body: some Scene {
    WindowGroup("Cox", id: Self.mainWindow) {
      SessionWindow(model: model).lendsOpenWindow(to: model).opensSpotlightResults()
    }
    .windowStyle(.hiddenTitleBar)
    .commands { ShellCommands() }
    // What needs you and what runs, from the menu bar, while `desktop.menu_bar` is on (T51.14).
    MenuBarExtra(isInserted: showsMenuBar) {
      MenuBarContent(model: model)
    } label: {
      Text(model.sidebar.inboxItems.isEmpty ? "cx" : "cx \(model.sidebar.inboxItems.count)")
        .lendsOpenWindow(to: model)
    }
    .menuBarExtraStyle(.window)
    // One session popped out of a window, alone or as a native tab (T51.11).
    WindowGroup("Session", for: PopOut.self) { $popOut in
      if let popOut { SessionWindow(model: model, popOut: popOut) }
    }
    .windowStyle(.hiddenTitleBar)
    Settings {
      SettingsWindow(model: model)
    }
  }
}

/// What every window of this launch shares: the core, one `SettingsStore` for the project, the
/// session list, the login shell's environment read once, and the notification centre's
/// delegate, which routes an action to the session it names.
@MainActor
final class AppModel {
  let launch: LaunchCore
  /// `nil` when the live core did not start; the windows say why.
  let settings: SettingsStore?
  /// The sidebar's sessions: the live workspace's, or a fixture's inbox alone.
  let sidebar: SidebarStore
  /// The open sessions every window shares (T51.11).
  let registry = AppStore()
  /// The remote hosts the sidebar lists (T52.21); a fixture launch connects none.
  let remotes: RemoteHosts
  /// A scene's `openWindow`, for the hotkeys and the intents, which fire outside every view
  /// (T51.15, T51.17); `nil` until the first window or the menu-bar label appears.
  private var openWindow: OpenWindowAction?
  /// Pop-outs asked for before any scene lent its `openWindow`, as when an intent launches cox.
  private var waiting: [PopOut] = []
  /// The listed sessions' titles in Spotlight (T51.16).
  private let spotlight = SpotlightIndex(store: CoreSpotlightStore())
  private var loginEnv: Task<Void, Never>?
  private var sessions: [String: WeakSession] = [:]
  private var responder: NotificationResponder?

  init(launch: LaunchCore) {
    self.launch = launch
    settings = try? SettingsStore(
      client: launch.live.get(), secrets: launch.secrets, catalog: launch.live.get(),
      cwd: LaunchCore.project())
    sidebar = SidebarStore(
      workspace: launch.isFixture ? nil : try? launch.live.get(),
      inbox: (try? launch.core.get() as? any InboxClient).map { InboxStore(client: $0) })
    remotes = RemoteHosts(
      connector: launch.isFixture ? nil : try? launch.live.get(), settings: settings)
    let responder = NotificationResponder(
      handle: { [weak self] route in Task { @MainActor in self?.route(route) } },
      show: { _ in Task { @MainActor in NSApp.activate() } })
    // The centre holds its delegate weakly; this model lives as long as the app.
    UNUserNotificationCenter.current().delegate = responder
    self.responder = responder
    Hotkeys.register(self)
    CoxIntents.register(self)
    sidebar.didRefresh = { [weak self] in self?.indexSessions() }
  }

  /// Mirrors the sidebar's last read into Spotlight: titles, projects and times only.
  private func indexSessions() {
    spotlight.sync(
      sidebar.listed.map {
        SpotlightRow(
          session: $0.session.id, title: $0.session.title ?? "", project: $0.project.name,
          lastActivity: $0.updated)
      })
  }

  /// Reads the login shell's environment into the process once per launch, before the first
  /// session opens (DT§4.8); every window waits on the same read. How it went is the checklist's
  /// shell-environment row.
  func loadLoginEnv() async {
    let read = loginEnv ?? Task { _ = try? await LiveCoreClient.loadLoginEnv() }
    loginEnv = read
    await read.value
  }

  /// Takes a scene's `openWindow` and opens what waited for one.
  func lend(_ action: OpenWindowAction) {
    openWindow = action
    for popOut in waiting { action(value: popOut) }
    waiting = []
  }

  /// Opens `popOut` now, or once a scene lends its `openWindow`.
  func show(_ popOut: PopOut) {
    guard let openWindow else { return waiting.append(popOut) }
    openWindow(value: popOut)
  }

  /// Opens a window of the scene `id`; nothing before a scene appeared, as launching opens one.
  func show(id: String) { openWindow?.callAsFunction(id: id) }

  /// Makes `store` the target of the notifications for `session`.
  func register(_ store: SessionStore, as session: String) {
    sessions[session] = WeakSession(store: store)
  }

  /// Allow, Deny or an answer from a notification or the menu bar, sent to the session it came
  /// from; a closed session's action is dropped, as its card is gone too.
  func route(_ route: NotificationRoute) {
    guard let store = sessions[route.session]?.store else { return }
    Task { _ = try? await store.send(route.intent) }
  }

  private struct WeakSession {
    weak var store: SessionStore?
  }
}
