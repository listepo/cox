// One window's sessions (DT§5.1): opens a session on the launch's core and shows it in CoxUI's
// `MainScreen`, the transcript and composer in its column, over the behind-window blur; the
// first-run checklist comes first on a first launch (DT§5.8), after the login shell's
// environment is read (DT§4.8). Wiring only — the stores decide and the packages draw. The
// sidebar lists the workspace's sessions and opens one here, the toolbar shows the open one's
// title, model, mode and cost and stops its turn, its model popover switches the session's model
// as `/model` does, the inspector's tabs read the open session,
// Review replaces the transcript column, the shell's panes fold and the Appearance popover writes
// `[desktop.appearance]`.

import CoxClient
import CoxModel
import CoxTranscript
import CoxUI
import SwiftUI

struct SessionWindow: View {
  let model: AppModel
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
  @Environment(\.coxAppearance) private var base

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
          toggleSidebar: { screen.isSidebarVisible.toggle() },
          toggleInspector: { screen.isInspectorVisible.toggle() })
      )
      .task { if current == nil { await open(resume: nil) } }
      .task { await watch() }
      .onDisappear {
        for session in opened.values { session.close() }
      }
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
      VStack(spacing: 0) {
        TranscriptView(store: showing.store, send: send)
          .composer(showing.composer)
        // At its own height, so the transcript takes the rest of the column.
        SessionComposer(store: showing.composer).fixedSize(horizontal: false, vertical: true)
      }
      // A new view per session, so the transcript's text is rebuilt from the one it shows.
      .id(current)
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
    case .showSidebar: screen.isSidebarVisible.toggle()
    case .toggleInspector: screen.isInspectorVisible.toggle()
    case .open(.appearance): screen.popover = screen.popover == .appearance ? nil : .appearance
    case .mode(let mode): send(.setMode(mode: PermissionMode(mode)))
    case .stop: send(.interrupt)
    case .open(.cost):
      // DT§5.1: the cost pill opens Context & Cost.
      (screen.inspectorTab, screen.isInspectorVisible) = (.context, true)
    case .open(.model): screen.popover = screen.popover == .model ? nil : .model
    }
  }

  private func handle(_ intent: Sidebar.Intent) {
    switch intent {
    case .hide: screen.isSidebarVisible.toggle()
    case .filter(let text): model.sidebar.filter = text
    case .toggle(let project): model.sidebar.toggle(project)
    case .newSession: Task { await open(resume: nil) }
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

  /// Opens a new session, or resumes `resume` where it last ran, and shows it.
  private func open(resume: String?) async {
    guard !isOpening else { return }
    isOpening = true
    defer { isOpening = false }
    do {
      let cwd = resume.flatMap { model.sidebar.entry($0)?.session.cwd } ?? LaunchCore.project()
      let client = try await model.launch.core.get().open(
        // The syntax theme the fixtures were recorded with; Settings' appearance replaces it.
        OpenSession(cwd: cwd, resume: resume, theme: "base16-ocean.dark"))
      let store = SessionStore(session: client)
      opened[client.id]?.close()
      opened[client.id] = OpenedSession(
        store: store, composer: ComposerStore(session: store), pull: Task { await store.run() })
      (current, failure, reviewing) = (client.id, nil, nil)
      opened[client.id]?.models = (try? model.launch.live.get().models(cwd: cwd)) ?? []
      model.sidebar.refresh()
      // After it shows: Info asks git about the cwd, which can take a while.
      if let info = try? await client.info() {
        model.register(store, as: info.session)
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
