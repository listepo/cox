// `ShellPane` (DS§4, DS§6.4 row `ShellPane`, the mockup's `.window`, `.sidebar`, `.col` and
// `.insp`): the glass layers the window shell is built from — the window over the wallpaper and
// the panes that float inside it with the wallpaper showing between them. Separate so every
// pane takes its shape, lift and surface from one table, and `MainScreen` composes panes
// without styling one (DS§5).

import SwiftUI

/// `content` on a chrome glass layer of `kind`: the material, window opacity and Depth come
/// from `coxAppearance` through `glassPane` and `elevation`, so `[desktop.appearance]` and
/// Reduce Transparency reach every pane alike.
struct ShellPane<Content: View>: View {
  let kind: ShellPaneKind
  let content: Content

  init(_ kind: ShellPaneKind, @ViewBuilder content: () -> Content) {
    self.kind = kind
    self.content = content()
  }

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: kind.radius, style: .continuous)
    // A pane fills the room it is given, even while its slot is still empty.
    ZStack(alignment: .top) {
      Color.clear
      content
    }
    .clipShape(shape)
    .glassPane(shape, surface: kind.surface)
    .hairline(in: shape)
    .elevation(kind.elevation, cornerRadius: kind.radius)
  }
}

/// The layers of the window shell.
enum ShellPaneKind: CaseIterable, Sendable {
  /// The window over the wallpaper, the highest lift (e5).
  case window
  /// The session list, lifted as a card (e2) on the sidebar surface.
  case sidebar
  /// The transcript column: flat glass, so the cards and composer inside it lift from it.
  case column
  /// The inspector, lifted as a card (e2).
  case inspector

  /// DS§3.3: the radius grows with the size of the thing.
  var radius: CGFloat {
    switch self {
    case .window: Radius.window
    case .sidebar: Radius.panel
    case .column, .inspector: Radius.pane
    }
  }

  /// DS§3.4: the window over the wallpaper, the side panes as cards; the column stays flat.
  var elevation: ElevationToken {
    switch self {
    case .window: .e5
    case .sidebar, .inspector: .e2
    case .column: .e0
    }
  }

  var surface: Color {
    switch self {
    case .sidebar: Color(.surfaceSidebar)
    case .window, .column, .inspector: Color(.surfaceWindow)
    }
  }
}

#Preview("panes") {
  PreviewMatrix {
    HStack(spacing: Size.paneGap) {
      ForEach(ShellPaneKind.allCases, id: \.self) { kind in
        ShellPane(kind) { Color.clear }.frame(width: Size.capsuleHeight * 2)
      }
    }
    .frame(height: Size.toolbarHeight * 2)
  }
}
