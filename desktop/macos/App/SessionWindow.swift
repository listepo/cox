// One window's sessions (DT§5.1): opens a session on the launch's core and shows it in CoxUI's
// `MainScreen`, the transcript and composer in its column, over the behind-window blur; the
// first-run checklist comes first on a first launch (DT§5.8), after the login shell's
// environment is read (DT§4.8). Wiring only — the stores decide and the packages draw. The
// sidebar lists the workspace's sessions and opens one here, the toolbar shows the open one's
// title, model, mode and cost and stops its turn, the toolbar's title and a sidebar row's menu
// rename a session (A113), its model popover switches the session's model
// as `/model` does, the inspector's tabs read the open session,
// Review replaces the transcript column, the shell's panes fold, ⌃` shows the session's terminal
// pane under the column (T51.6; the window asks before closing over a running command), ⌘⇧B
// shows the browser pane beside it (T51.10), plugin panels sit above the composer and a plugin
// overlay shows as a sheet (T52.17), and the Appearance popover writes `[desktop.appearance]`.
// A popped-out window (T51.11) is the same view on one session with no sidebar; every window
// on a session shares its stores through `AppStore`.

import CoxClient
import CoxModel
import CoxTranscript
import CoxUI
import SwiftUI

struct SessionWindow: View {
  let model: AppModel
  /// Set for a popped-out window: the one session it shows, and the window it joins as a tab.
  let popOut: PopOut?
  /// The syntax theme the fixtures were recorded with; Settings' appearance replaces it.
  static let syntaxTheme = "base16-ocean.dark"
  /// This window's hold on the sessions it shows in `AppStore`.
  @State private var windowID = UUID()
  /// Set once first run chose a project; a fixture launch never asks.
  @AppStorage("CoxOnboarded") private var onboarded = false
  @State private var screen = MainScreenState()
  @State private var appearanceWrites = Coalescer()
  /// Every session this window opened, by id; each keeps pulling while another shows.
  @State private var opened: [String: OpenedSession] = [:]
  /// The one the window shows.
  @State private var current: String?
  @State private var isOpening = false
  /// The login shell's environment is in the process, so sessions may open.
  @State private var isEnvLoaded = false
  /// The checklist's provider-key row, for the sidebar's footer dot.
  @State private var providerCheck: CheckRow?
  /// The providers a turn could run on now, for the footer's count (A110); nil until probed.
  @State private var usable: [String]?
  /// Review shows in the column instead of the transcript, at this file or the first changed one.
  @State private var reviewing: Reviewing?
  /// Why the first session did not open.
  @State private var failure: String?
  /// Why the core refused the last intent; shown until dismissed.
  @State private var refused: String?
  /// The terminal pane shows under the column; its height is the user's drag, UI-only.
  @State private var isTerminalVisible = false
  @State private var terminalHeight = SessionTerminal.defaultHeight
  /// The browser pane shows beside the column; UI-only, like the terminal's.
  @State private var isBrowserVisible = false
  @Environment(\.coxAppearance) private var base
  @Environment(\.openWindow) private var openWindow

  init(model: AppModel, popOut: PopOut? = nil) {
    self.model = model
    self.popOut = popOut
    var screen = MainScreenState()
    screen.isSidebarVisible = popOut == nil
    _screen = State(initialValue: screen)
  }

  var body: some View {
    Group {
      if !isEnvLoaded {
        ProgressView().task {
          await model.loadLoginEnv()
          isEnvLoaded = true
        }
      } else if !onboarded && !model.launch.isFixture {
        FirstRun(launch: model.launch) { onboarded = true }
      } else {
        main
      }
    }
    .environment(\.coxAppearance, screen.appearance.applied(to: base))
    .behindWindowBlur(
      screen.appearance.blurFraction, tint: screen.appearance.tint,
      in: RoundedRectangle(cornerRadius: Radius.window, style: .continuous)
    )
    .seeThroughWindow()
    .task { await readSettings() }
    .onChange(of: model.settings?.view) {
      if !appearanceWrites.isPending { readAppearance() }
    }
    .alert(refused ?? "", isPresented: isRefused) {}
  }

  private var main: some View {
    MainScreen(state: shown, send: handle, transcript: { column }, inspector: { inspector($0) })
      .focusedSceneValue(
        \.shell,
        ShellActions(
          isSidebarVisible: screen.isSidebarVisible, isInspectorVisible: screen.isInspectorVisible,
          isTerminalVisible: isTerminalShown, isBrowserVisible: isBrowserVisible,
          toggleSidebar: { toggleSidebar() },
          toggleInspector: { screen.isInspectorVisible.toggle() },
          toggleTerminal: { toggleTerminal() }, toggleBrowser: { isBrowserVisible.toggle() },
          popOut: current.map { session -> (Bool) -> Void in { openPopOut(session, asTab: $0) } })
      )
      .task { if current == nil { await open(resume: popOut?.session) } }
      .task { await watch() }
      .onDisappear {
        for session in opened.values { session.close(in: model.registry, window: windowID) }
      }
      .joinsTabs(of: popOut?.tabOf)
      .closeGuard { opened.values.contains { $0.store.hasBusyTerminal } }
  }

  private var shown: MainScreenState {
    var state = screen
    state.toolbar = ShellState.toolbar(showing, sidebar: model.sidebar, popover: screen.popover)
    state.model = ShellState.models(showing?.menu)
    state.sidebar = ShellState.sidebar(
      model.sidebar, selection: current,
      providers: ProviderHealth(usable: usable, check: providerCheck))
    return state
  }

  private var showing: OpenedSession? { current.flatMap { opened[$0] } }

  /// The pane shows while it is toggled on and the session has a terminal left open.
  private var isTerminalShown: Bool {
    isTerminalVisible && showing?.store.terminals.isEmpty == false
  }

  /// ⌃`: shows or hides the terminal pane; showing it with no terminal open opens the session's
  /// shell first.
  private func toggleTerminal() {
    guard let store = showing?.store else { return }
    let isShown = isTerminalShown
    if !isShown && store.terminals.isEmpty {
      do {
        try store.openTerminal()
      } catch {
        refused = String(describing: error)
        return
      }
    }
    isTerminalVisible = !isShown
  }

  /// A popped-out window has no sidebar to show.
  private func toggleSidebar() {
    if popOut == nil { screen.isSidebarVisible.toggle() }
  }

  /// Opens `session` in a window of its own, or as a tab of this one.
  private func openPopOut(_ session: String, asTab: Bool) {
    openWindow(value: PopOut(session: session, asTab: asTab))
  }

  /// A plugin overlay shows as a sheet while the core says it is shown; Esc, which dismisses the
  /// sheet, hides it in the core too, so the next patch agrees.
  private func pluginOverlay(_ store: SessionStore) -> Binding<Bool> {
    Binding(
      get: { PluginWidgets.overlay(store) != nil },
      set: { if !$0 { store.closePluginOverlay() } })
  }

  private var isRefused: Binding<Bool> {
    Binding(get: { refused != nil }, set: { if !$0 { refused = nil } })
  }

  @ViewBuilder private var column: some View {
    if let showing, let reviewing {
      SessionReview(
        store: showing.store, path: reviewing.path,
        reviewSend: model.settings?.reviewSend ?? .queue
      ) { refused = $0 }
      .onExitCommand { self.reviewing = nil }
    } else if let showing {
      HStack(spacing: 0) {
        VStack(spacing: 0) {
          TranscriptView(store: showing.store, send: send)
            .composer(showing.composer)
          let panels = PluginWidgets.panels(showing.store)
          if !panels.isEmpty { PluginPanel(panels).fixedSize(horizontal: false, vertical: true) }
          // At its own height, so the transcript takes the rest of the column.
          SessionComposer(store: showing.composer).fixedSize(horizontal: false, vertical: true)
          if isTerminalShown {
            SessionTerminal(
              store: showing.store, surfaces: showing.terminals,
              branch: showing.info?.worktree?.branch, height: $terminalHeight
            ) { refused = $0 }
          }
        }
        if isBrowserVisible {
          SessionBrowser(controller: model.launch.browser) { refused = $0 }
            .frame(width: SessionBrowser.paneWidth)
        }
      }
      // A new view per session, so the transcript's text is rebuilt from the one it shows.
      .id(current)
      .sheet(isPresented: pluginOverlay(showing.store)) {
        if let overlay = PluginWidgets.overlay(showing.store) {
          ScrollView { PluginWidgetView(overlay).padding(Space.xl) }
            .frame(minWidth: Size.readingWidth, minHeight: Size.popoverWidth)
        }
      }
    } else if let failure {
      Text(failure).textSelection(.enabled)
    } else {
      ProgressView()
    }
  }

  @ViewBuilder private func inspector(_ tab: InspectorTab) -> some View {
    if let showing {
      SessionInspector(
        store: showing.store, tab: tab, cacheHit: model.settings?.cacheHitScope ?? .turn
      ) { request in
        switch request {
        // The tab's Review button shows Review, or hides it again.
        case .review(nil): reviewing = reviewing == nil ? Reviewing(path: nil) : nil
        case .review(let path): reviewing = Reviewing(path: path)
        case .open(let session): handle(Sidebar.Intent.select(session))
        case .refused(let why): refused = why
        }
      }
      .id(current)
    }
  }

  /// What the shell reports: the panes fold here, and an appearance change shows at once and is
  /// written once the control rests.
  private func handle(_ intent: MainScreenIntent) {
    switch intent {
    case .sidebar(let intent): handle(intent)
    case .toolbar(let intent): handle(intent)
    case .dismissPopover: screen.popover = nil
    case .inspectorTab(let tab): screen.inspectorTab = tab
    case .model(let row):
      screen.popover = nil
      if let intent = showing?.menu.pick(row) { send(intent) }
    case .appearance(let change):
      screen.appearance.apply(change)
      screen.appearance.fillTexts()
      let edit = AppearanceEdit(change)
      guard let settings = model.settings else { return }
      appearanceWrites.submit(edit.key) { await settings.apply(edit) }
    }
  }

  private func handle(_ intent: SessionToolbar.Intent) {
    switch intent {
    case .showSidebar: toggleSidebar()
    case .toggleInspector: screen.isInspectorVisible.toggle()
    case .open(.appearance): screen.popover = screen.popover == .appearance ? nil : .appearance
    case .mode(let mode): send(.setMode(mode: PermissionMode(mode)))
    case .stop: send(.interrupt)
    case .open(.cost):
      // DT§5.1: the cost pill opens Context & Cost.
      (screen.inspectorTab, screen.isInspectorVisible) = (.context, true)
    case .open(.model): screen.popover = screen.popover == .model ? nil : .model
    case .rename(let title): send(.rename(title: title))
    }
  }

  private func handle(_ intent: Sidebar.Intent) {
    switch intent {
    case .hide: toggleSidebar()
    case .filter(let text): model.sidebar.filter = text
    case .toggle(let project): model.sidebar.toggle(project)
    case .newSession: Task { await open(resume: nil) }
    case .popOut(let session, let asTab): openPopOut(session, asTab: asTab)
    case .rename(let session, let title):
      // An open session renames through its core; the store takes a closed one's directly.
      if let store = opened[session]?.store {
        send(.rename(title: title), to: store)
      } else {
        model.sidebar.rename(session, to: title)
      }
    case .select(let session):
      reviewing = nil
      if opened[session] != nil {
        current = session
      } else if !model.launch.isFixture {
        // A recording replays one session; its inbox rows name sessions it cannot open.
        Task { await open(resume: session) }
      }
    }
  }

  /// Reads the footer's provider health, then keeps the session list current while the window
  /// is open: this and other processes add sessions, and the inbox changes as turns run.
  private func watch() async {
    if let live = try? model.launch.live.get() {
      let cwd = LaunchCore.project()
      providerCheck = try? await live.checklist(cwd: cwd).first { $0.id == .providerKey }
      usable = try? await live.usableProviders(cwd: cwd)
    }
    await model.sidebar.watch()
  }

  private func readSettings() async {
    appearanceWrites.onIdle = { readAppearance() }
    await model.settings?.load()
    readAppearance()
  }

  private func readAppearance() {
    guard let settings = model.settings else { return }
    screen.appearance = AppearancePopover.State(settings)
  }

  /// Opens a new session, or resumes `resume` where it last ran, and shows it. A session another
  /// window already shows is joined, not opened again.
  private func open(resume: String?) async {
    guard !isOpening else { return }
    isOpening = true
    defer { isOpening = false }
    do {
      let cwd = resume.flatMap { model.sidebar.entry($0)?.session.cwd } ?? LaunchCore.project()
      let shared: AppStore.Shared
      if let resume, let joined = model.registry.join(resume, window: windowID) {
        shared = joined
      } else {
        let client = try await model.launch.core.get().open(
          OpenSession(cwd: cwd, resume: resume, theme: Self.syntaxTheme))
        shared = model.registry.adopt(client, window: windowID)
      }
      let client = shared.store.session
      // An asked session was held for this window (T51.17); the window holds it now.
      if let handoff = popOut?.handoff { model.registry.release(client.id, window: handoff) }
      if opened[client.id] == nil { opened[client.id] = OpenedSession(shared) }
      (current, failure, reviewing) = (client.id, nil, nil)
      opened[client.id]?.models = (try? model.launch.live.get().models(cwd: cwd)) ?? []
      model.sidebar.refresh()
      // After it shows: Info asks git about the cwd, which can take a while.
      if let info = try? await client.info() {
        model.register(shared.store, as: info.session)
        opened[client.id]?.info = info
      }
    } catch {
      // Without a first session the column says why; later, an alert does.
      if current == nil {
        failure = String(describing: error)
      } else {
        refused = String(describing: error)
      }
    }
  }

  private func send(_ intent: Intent) {
    guard let store = showing?.store else { return }
    send(intent, to: store)
  }

  private func send(_ intent: Intent, to store: SessionStore) {
    Task {
      do {
        _ = try await store.send(intent)
      } catch {
        refused = String(describing: error)
      }
    }
  }
}

/// Where Review opened.
private struct Reviewing: Equatable {
  var path: String?
}
