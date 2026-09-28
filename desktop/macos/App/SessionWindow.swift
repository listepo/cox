// One window's session (DT§5.1): opens a session on the launch's core and shows it in CoxUI's
// `MainScreen`, the transcript and composer in its column. Wiring only — the stores decide and
// the packages draw. The shell's panes show their defaults and their intents go nowhere until
// T37.22.3 binds them to the stores.

import CoxClient
import CoxModel
import CoxTranscript
import SwiftUI
// `MainScreen` and its state and intent types are internal to CoxUI until T37.22.3 makes the
// shell's API public; a Debug build compiles the packages testable, so this reaches them.
@testable import CoxUI

struct SessionWindow: View {
  let core: Result<any CoreClient, any Error>
  @State private var session: SessionStore?
  @State private var composer: ComposerStore?
  /// Why the session did not open.
  @State private var failure: String?
  /// Why the core refused the last intent; shown until dismissed.
  @State private var refused: String?

  var body: some View {
    MainScreen(
      state: MainScreenState(), send: { _ in }, transcript: { column },
      inspector: { _ in EmptyView() }
    )
    .task { await open() }
    .alert(refused ?? "", isPresented: isRefused) {}
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

  private func open() async {
    do {
      let client = try await core.get().open(
        // The syntax theme the fixtures were recorded with; Settings' appearance replaces it.
        OpenSession(cwd: LaunchCore.project(), theme: "base16-ocean.dark"))
      let store = SessionStore(session: client)
      (session, composer) = (store, ComposerStore(session: store))
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
