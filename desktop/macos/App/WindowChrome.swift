// The session window's chrome (DS§3.5, DS§4): no title bar, a see-through window, and the
// behind-window blur under `MainScreen`'s window pane that draws `[desktop.appearance]`'s blur
// and wallpaper tint. AppKit, so it lives in the app: CoxUI draws the panes, and only the window
// can see the desktop behind it.

import AppKit
import SwiftUI

/// The desktop behind the window, blurred by `NSVisualEffectView`. Its strength is `blur`, 0…1,
/// shown as the view's opacity over the plain wallpaper, since AppKit offers no blur radius; with
/// `tint` off the view drops its colour, so the wallpaper's hue does not tint the window.
struct BehindWindowBlur: NSViewRepresentable {
  var blur: Double
  var tint: Bool

  func makeNSView(context: Context) -> NSVisualEffectView {
    let view = NSVisualEffectView()
    view.blendingMode = .behindWindow
    // The lightest see-through material: the wallpaper's colour carries through its frost, as
    // the mockup's `.window` blur does, where `.underWindowBackground` greys it out.
    view.material = .fullScreenUI
    view.state = .followsWindowActiveState
    return view
  }

  func updateNSView(_ view: NSVisualEffectView, context: Context) {
    view.alphaValue = blur
  }
}

extension View {
  /// Spreads this view over the whole window, the title bar strip included, on the desktop
  /// behind the window blurred in `shape`: no strip of the window shows the desktop sharp.
  func behindWindowBlur(_ blur: Double, tint: Bool, in shape: some Shape) -> some View {
    ignoresSafeArea().background {
      BehindWindowBlur(blur: blur, tint: tint)
        .saturation(tint ? 1 : 0)
        .clipShape(shape)
        .ignoresSafeArea()
    }
  }

  /// Makes the hosting window see-through, so the panes' glass shows the desktop (DS§4).
  func seeThroughWindow() -> some View { background(SeeThroughWindow()) }
}

/// Clears the window's own background once the view is in it; the title bar is hidden by the
/// scene's `.hiddenTitleBar` style. An empty unified toolbar makes the title bar the height of
/// the sidebar's top row, so the system centres the window buttons inside the sidebar pane, off
/// the window's edge, as the mockup's `.traffic` row sits.
private struct SeeThroughWindow: NSViewRepresentable {
  func makeNSView(context: Context) -> Probe { Probe() }
  func updateNSView(_ view: Probe, context: Context) {}

  final class Probe: NSView {
    override func viewDidMoveToWindow() {
      super.viewDidMoveToWindow()
      guard let window else { return }
      window.isOpaque = false
      window.backgroundColor = .clear
      window.titlebarAppearsTransparent = true
      window.titleVisibility = .hidden
      window.titlebarSeparatorStyle = .none
      if window.toolbar == nil { window.toolbar = NSToolbar(identifier: "CoxWindowButtons") }
      window.toolbarStyle = .unified
    }
  }
}
