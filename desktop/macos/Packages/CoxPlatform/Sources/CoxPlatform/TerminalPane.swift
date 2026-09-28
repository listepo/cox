// The terminal pane's view (T51.5, DT§3.2, mockup 24): SwiftTerm's `TerminalView` — the
// emulator and renderer only, never `LocalProcessTerminalView`, because Swift never spawns a
// process (DT§4.6) — bridged to a `TerminalClient`. Keys and pastes the view encodes go to
// `write`, its new size in cells to `resize`, and one task feeds the shell's bytes in. In
// CoxPlatform because it is the one package that may link a platform library (DT§4.6); the
// colours and font arrive as a `TerminalStyle` the app builds from CoxUI's tokens.

import AppKit
import CoxClient
@preconcurrency import SwiftTerm
import SwiftUI

/// How the pane draws: the mono font token and the `surface.terminal`, `text.terminal` and
/// `text.terminalOk` colours, resolved by the caller.
public struct TerminalStyle {
  public var font: NSFont
  public var foreground: NSColor
  public var background: NSColor
  public var caret: NSColor

  public init(font: NSFont, foreground: NSColor, background: NSColor, caret: NSColor) {
    (self.font, self.foreground, self.background, self.caret) = (
      font, foreground, background, caret
    )
  }

  @MainActor func apply(to view: TerminalView) {
    if view.font != font { view.font = font }
    view.nativeForegroundColor = foreground
    view.nativeBackgroundColor = background
    view.caretColor = caret
  }
}

/// One terminal in SwiftUI. `openLink` receives a link the user clicked in the output;
/// by default nothing opens, since the host opens web links only after its own check.
public struct TerminalPane: NSViewRepresentable {
  let client: any TerminalClient
  let style: TerminalStyle
  let openLink: @MainActor (String) -> Void

  public init(
    client: any TerminalClient, style: TerminalStyle,
    openLink: @escaping @MainActor (String) -> Void = { _ in }
  ) {
    (self.client, self.style, self.openLink) = (client, style, openLink)
  }

  public func makeCoordinator() -> TerminalBridge {
    TerminalBridge(client: client, openLink: openLink)
  }

  public func makeNSView(context: Context) -> TerminalView {
    let view = TerminalView(frame: .zero, font: style.font)
    style.apply(to: view)
    context.coordinator.attach(view)
    return view
  }

  public func updateNSView(_ view: TerminalView, context: Context) {
    style.apply(to: view)
  }

  public static func dismantleNSView(_ view: TerminalView, coordinator: TerminalBridge) {
    coordinator.detach()
  }
}

/// The delegate between the view and the client, and the task that feeds the view.
@MainActor
public final class TerminalBridge {
  let client: any TerminalClient
  let openLink: @MainActor (String) -> Void
  private var feed: Task<Void, Never>?

  init(client: any TerminalClient, openLink: @escaping @MainActor (String) -> Void) {
    (self.client, self.openLink) = (client, openLink)
  }

  /// Makes this the view's delegate and starts feeding it the shell's output.
  func attach(_ view: TerminalView) {
    view.terminalDelegate = self
    feed?.cancel()
    feed = Task { [client, weak view] in
      for await bytes in client.outputs {
        guard let view, !Task.isCancelled else { return }
        view.feed(byteArray: bytes[...])
      }
    }
  }

  /// Stops feeding; the pane's owner decides whether the shell closes.
  func detach() {
    feed?.cancel()
    feed = nil
  }
}

extension TerminalBridge: @preconcurrency TerminalViewDelegate {
  public func send(source: TerminalView, data: ArraySlice<UInt8>) {
    // A write fails only once the shell is gone; the view shows its end.
    try? client.write(Array(data))
  }

  public func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
    try? client.resize(cols: UInt16(clamping: newCols), rows: UInt16(clamping: newRows))
  }

  public func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
    openLink(link)
  }

  public func setTerminalTitle(source: TerminalView, title: String) {}

  public func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}

  public func scrolled(source: TerminalView, position: Double) {}

  public func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}

  /// No program in the pane may read the user's clipboard (OSC 52).
  public func clipboardRead(source: TerminalView) -> Data? { nil }
}
