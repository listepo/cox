// One window's session (DT§5.1): opens a session on the launch's core and shows it in CoxUI's
// `MainScreen`, the transcript and composer in its column, over the behind-window blur; the
// first-run checklist comes first on a first launch (DT§5.8). Wiring only — the stores decide
// and the packages draw. The shell's panes fold and the Appearance popover writes
// `[desktop.appearance]` here; the inspector tabs still show nothing.

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
  @State private var session: SessionStore?
  @State private var composer: ComposerStore?
  /// Why the session did not open.
  @State private var failure: String?
  /// Why the core refused the last intent; shown until dismissed.
  @State private var refused: String?
  @Environment(\.coxAppearance) private var base

  var body: some View {
    Group {
      if !onboarded && !model.launch.isFixture {
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
    MainScreen(state: screen, send: handle, transcript: { column }, inspector: { _ in EmptyView() })
      .focusedSceneValue(
        \.shell,
        ShellActions(
          isSidebarVisible: screen.isSidebarVisible, isInspectorVisible: screen.isInspectorVisible,
          toggleSidebar: { screen.isSidebarVisible.toggle() },
          toggleInspector: { screen.isInspectorVisible.toggle() })
      )
      .task { await open() }
  }

  private var isRefused: Binding<Bool> {
    Binding(get: { refused != nil }, set: { if !$0 { refused = nil } })
  }

  @ViewBuilder private var column: some View {
    if let session, let composer {
      VStack(spacing: 0) {
        TranscriptView(store: session, send: { send($0, to: session) })
          .composer(composer)
        // At its own height, so the transcript takes the rest of the column.
        SessionComposer(store: composer).fixedSize(horizontal: false, vertical: true)
      }
    } else if let failure {
      Text(failure).textSelection(.enabled)
    } else {
      ProgressView()
    }
  }

  /// What the shell reports: the panes fold here, and an appearance change shows at once and is
  /// written once the control rests.
  private func handle(_ intent: MainScreenIntent) {
    switch intent {
    case .sidebar(.hide), .toolbar(.showSidebar): screen.isSidebarVisible.toggle()
    case .toolbar(.toggleInspector): screen.isInspectorVisible.toggle()
    case .toolbar(.open(.appearance)):
      screen.popover = screen.popover == .appearance ? nil : .appearance
    case .dismissPopover: screen.popover = nil
    case .inspectorTab(let tab): screen.inspectorTab = tab
    case .appearance(let change):
      screen.appearance.apply(change)
      screen.appearance.fillTexts()
      let edit = AppearanceEdit(change)
      guard let settings = model.settings else { return }
      appearanceWrites.submit(edit.key) { await settings.apply(edit) }
    case .sidebar, .toolbar:
      // The session list, the model and cost popovers, the mode and Stop are bound with the
      // sidebar's and toolbar's rows.
      break
    }
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

  private func open() async {
    do {
      let client = try await model.launch.core.get().open(
        // The syntax theme the fixtures were recorded with; Settings' appearance replaces it.
        OpenSession(cwd: LaunchCore.project(), theme: "base16-ocean.dark"))
      let store = SessionStore(session: client)
      (session, composer) = (store, ComposerStore(session: store))
      if let info = try? await client.info() { model.register(store, as: info.session) }
      await store.run()
    } catch {
      failure = String(describing: error)
    }
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
