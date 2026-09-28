// First run (DT§5.8, T37.31): cox-app's checklist in CoxUI's `OnboardingScreen`, the folder the
// first session opens in, and the way to Settings for a missing key. Shown in the session window
// until a project is chosen; the choice is kept, so later launches go straight to the session.

import AppKit
import CoxClient
import CoxCore
import CoxUI
import SwiftUI

struct FirstRun: View {
  let launch: LaunchCore
  /// A project was chosen.
  let done: () -> Void
  @State private var state = OnboardingScreenState()
  @Environment(\.openSettings) private var openSettings

  var body: some View {
    OnboardingScreen(state: state) { intent in
      switch intent {
      case .chooseFolder: choose()
      case .openSettings: openSettings()
      case .retry: Task { await check() }
      }
    }
    .task { await check() }
  }

  private func check() async {
    do {
      let rows = try await launch.live.get().checklist(cwd: LaunchCore.project())
      state.checks = rows.map(Self.check)
    } catch {
      // Without the core nothing else can be checked; say why and offer to try again.
      state.checks = [
        .init(
          id: "core", title: "Cox core", detail: String(describing: error), status: .missing,
          fix: .retry)
      ]
    }
  }

  private func choose() {
    let panel = NSOpenPanel()
    (panel.canChooseDirectories, panel.canChooseFiles) = (true, false)
    panel.prompt = "Open"
    guard panel.runModal() == .OK, let folder = panel.url else { return }
    UserDefaults.standard.set(folder.path(percentEncoded: false), forKey: LaunchCore.projectKey)
    done()
  }

  /// A row as the checklist shows it: a missing key sends the person to Settings; anything else
  /// is fixed outside the app, then checked again.
  private static func check(_ row: CheckRow) -> OnboardingScreen.Check {
    let title =
      switch row.id {
      case .providerKey: "Provider key"
      case .git: "Git"
      case .sandbox: "Sandbox"
      case .shellEnv: "Shell environment"
      }
    let status: ChecklistRow.Status =
      switch row.status {
      case .passed: .passed
      case .warning: .warning
      case .failed: .missing
      }
    let fix: OnboardingScreen.Fix? =
      row.status == .passed ? nil : row.id == .providerKey ? .openSettings : .retry
    return .init(id: row.id.rawValue, title: title, detail: row.detail, status: status, fix: fix)
  }
}
