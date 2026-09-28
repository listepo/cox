// The session window's chrome (DS§3.5, DS§4): no title bar, a see-through window, and the
// behind-window blur under `MainScreen`'s window pane that draws `[desktop.appearance]`'s blur
// and wallpaper tint. AppKit, so it lives in the app: CoxUI draws the panes, and only the window
// can see the desktop behind it.

import AppKit
import SwiftUI

/// The desktop behind the window, blurred by `NSVisualEffectView`. Its strength is `blur`, 0…1
/// of the schema's range, shown as the view's opacity over the plain wallpaper, since AppKit
/// offers no blur radius; with `tint` off the view drops its colour, so the wallpaper's hue
/// does not tint the window.
struct BehindWindowBlur: NSViewRepresentable {
  var blur: Double
  var tint: Bool

  func makeNSView(context: Context) -> NSVisualEffectView {
    let view = NSVisualEffectView()
    view.blendingMode = .behindWindow
    view.material = .underWindowBackground
    view.state = .followsWindowActiveState
    return view
  }

  func updateNSView(_ view: NSVisualEffectView, context: Context) {
    view.alphaValue = blur
  }
}

extension View {
  /// The desktop behind the window blurred in `shape`, under this view.
  func behindWindowBlur(_ blur: Double, tint: Bool, in shape: some Shape) -> some View {
    background {
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
/// scene's `.hiddenTitleBar` style, and the system window buttons stay over the sidebar.
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
    }
  }
}
