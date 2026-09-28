// `SettingsSidebar` (DS§6.4 row `SettingsSidebar`, the mockup's `.set-side`; DT§5.7): the
// Settings window's page list — General through Advanced — and, at its foot, the config files
// the values come from. Separate so the Settings screen composes its sidebar like `MainScreen`
// composes the session list, and reports a page choice as an intent.

import SwiftUI

/// The Settings pages in DT§5.7's order, each with its title and DS§3.7 symbol.
enum SettingsPage: String, CaseIterable, Identifiable, Sendable {
  case general, models, permissions, sandbox, budget, mcp, plugins, appearance, advanced

  var id: Self { self }

  var title: String {
    switch self {
    case .general: "General"
    case .models: "Models & Providers"
    case .permissions: "Permissions"
    case .sandbox: "Sandbox"
    case .budget: "Budget"
    case .mcp: "MCP Servers"
    case .plugins: "Plugins"
    case .appearance: "Appearance"
    case .advanced: "Advanced"
    }
  }

  var symbol: String {
    switch self {
    case .general: "gearshape"
    case .models: "sparkle"
    case .permissions: "shield"
    case .sandbox: "lock"
    case .budget: "dollarsign.circle"
    case .mcp: "powerplug"
    case .plugins: "cpu"
    case .appearance: "paintbrush"
    case .advanced: "slider.horizontal.3"
    }
  }
}

/// `pages` as `InspectorRow`s on a `ShellPane(.sidebar)`, the selected one lifted, over the
/// user file and the project file with the layer badge each one sets.
struct SettingsSidebar: View {
  let pages: [SettingsPage]
  let selection: SettingsPage
  let userFile: String
  let projectFile: String?
  let select: (SettingsPage) -> Void

  var body: some View {
    ShellPane(.sidebar) {
      VStack(alignment: .leading, spacing: 0) {
        // The system's window buttons sit in this row.
        Spacer().frame(height: Size.toolbarHeight - Size.paneGap)
        ScrollView {
          VStack(spacing: Space.xxs) {
            ForEach(pages) { page in
              Button {
                select(page)
              } label: {
                InspectorRow(symbol: page.symbol, isSelected: page == selection, actions: []) {
                  Text(page.title).frame(maxWidth: .infinity, alignment: .leading)
                }
              }
              .buttonStyle(.plain)
            }
          }
          .padding(.horizontal, Space.m)
        }
        VStack(alignment: .leading, spacing: Space.xs) {
          ConfigFile(path: userFile, source: .user)
          if let projectFile { ConfigFile(path: projectFile, source: .project) }
        }
        .padding(Space.l)
        .hairline(.top)
      }
    }
    .frame(width: Size.sidebarWidth)
  }
}

/// A config file's path, cut in the middle, and the badge of the layer it sets.
private struct ConfigFile: View {
  let path: String
  let source: SettingSource

  var body: some View {
    HStack(spacing: Space.s) {
      Text(path)
        .textStyle(.footnote)
        .foregroundStyle(Color(.textSecondary))
        .lineLimit(1)
        .truncationMode(.middle)
        .frame(maxWidth: .infinity, alignment: .leading)
      Badge(source.name, kind: source.badge)
    }
    .accessibilityElement(children: .combine)
  }
}

#Preview("models") {
  PreviewMatrix {
    SettingsSidebar(
      pages: SettingsPage.allCases, selection: .models, userFile: PreviewState.userFile,
      projectFile: PreviewState.projectFile
    ) { _ in }
    .frame(height: Size.windowMinHeight)
  }
}
