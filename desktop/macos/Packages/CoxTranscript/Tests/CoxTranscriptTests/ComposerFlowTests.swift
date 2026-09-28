// T37.24's Check: a `SessionComposer` in a window, driven by real key events — type `@`, pick a
// file from the rows with ↓ and ⏎, type the rest, send with ⏎ — and the intent reaches the
// fixture client, which answers the completion without Rust.

import AppKit
import CoxClient
import CoxModel
import CoxTranscript
import CoxUI
import SwiftUI
import Testing

@MainActor
@Suite struct ComposerFlowTests {
  @Test func typingAtPickingAFileAndSendingReachesTheClient() async throws {
    let rows = [
      Completion(insert: "@src/lib.rs", detail: "src/lib.rs"),
      Completion(insert: "@src/main.rs", detail: "src/main.rs"),
    ]
    let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []), completions: rows)
    let store = ComposerStore(session: SessionStore(session: session))
    let host = ComposerHost(SessionComposer(store: store))
    defer { host.close() }

    host.type("@")
    #expect(store.completions.map(\.insert) == rows.map(\.insert))
    host.press(.down)
    host.press(.return)
    #expect(store.text == "@src/main.rs ")
    #expect(store.mentions == ["@src/main.rs"])

    host.type("explain it")
    host.press(.return)
    await host.settle(until: { !session.sent.isEmpty })
    #expect(session.sent == [.send(text: "@src/main.rs explain it", attachments: [])])
    #expect(host.editor.string.isEmpty)
  }

  @Test func whileATurnRunsReturnQueuesAndCommandReturnInterruptsAndSends() async throws {
    let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
    let transcript = SessionStore(session: session)
    let tally = Tally(
      sent: 0, received: 0, cacheRead: 0, cacheWrite: 0, uncached: 0, costUsd: 0, calls: 0,
      estimated: false)
    let running = TurnUsage(
      turn: "t1", tally: tally, thinkingTokens: 0, ttftMs: nil, tokPerS: nil, exact: false,
      sparkline: [], done: false)
    transcript.apply([.usage(usage: UsageView(session: tally, turn: running, contextTokens: 0))])
    let store = ComposerStore(session: transcript)
    let host = ComposerHost(SessionComposer(store: store))
    defer { host.close() }

    host.type("next")
    host.press(.return)
    await host.settle(until: { !session.sent.isEmpty })
    #expect(session.sent == [.queue(text: "next")])
    #expect(store.queued == 1)

    host.type("now")
    host.press(.commandReturn)
    await host.settle(until: { session.sent.count == 3 })
    #expect(session.sent.suffix(2) == [.interrupt, .send(text: "now", attachments: [])])
  }
}

/// A view in a borderless window far off screen, with its text view first responder, so key
/// events travel the path a keyboard's do.
@MainActor
private final class ComposerHost {
  let window: NSWindow
  let editor: NSTextView

  enum Key {
    case down, `return`, commandReturn

    var code: UInt16 { self == .down ? 125 : 36 }
    var characters: String { self == .down ? "\u{F701}" : "\r" }
    var modifiers: NSEvent.ModifierFlags {
      switch self {
      case .down: [.numericPad, .function]
      case .return: []
      case .commandReturn: .command
      }
    }
  }

  init(_ view: some View) {
    NSApplication.shared.setActivationPolicy(.accessory)
    // Room above the composer for the rows it floats there.
    let hosting = NSHostingView(
      rootView: view.frame(width: Size.readingWidth).padding(Size.toolbarHeight * 4))
    window = NSWindow(
      // A window frame far off screen, not a design size.
      // swiftlint:disable:next no_literal_size
      contentRect: NSRect(x: -20_000, y: -20_000, width: 1_100, height: 600),
      styleMask: [.borderless], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = hosting
    window.orderFrontRegardless()
    window.layoutIfNeeded()
    editor = Self.textViews(in: hosting).first ?? NSTextView()
    window.makeFirstResponder(editor)
    settle()
  }

  func type(_ text: String) {
    editor.insertText(text, replacementRange: editor.selectedRange())
    settle()
  }

  func press(_ key: Key) {
    let event = NSEvent.keyEvent(
      with: .keyDown, location: .zero, modifierFlags: key.modifiers,
      timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
      context: nil, characters: key.characters, charactersIgnoringModifiers: key.characters,
      isARepeat: false, keyCode: key.code)
    if let event { window.sendEvent(event) }
    settle()
  }

  /// A few turns of the run loop, so SwiftUI applies what the store changed.
  func settle() {
    for _ in 0..<5 {
      window.layoutIfNeeded()
      RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.02))
    }
  }

  /// Yields to the tasks the view started — a send runs in one — until `done` holds or `limit`
  /// passes, then lets SwiftUI apply what they changed.
  func settle(until done: () -> Bool, limit: Duration = .seconds(5)) async {
    let deadline = ContinuousClock.now + limit
    while !done(), ContinuousClock.now < deadline { try? await Task.sleep(for: .milliseconds(20)) }
    settle()
  }

  func close() {
    window.orderOut(nil)
    window.close()
  }

  private static func textViews(in view: NSView) -> [NSTextView] {
    var found: [NSTextView] = []
    var stack: [NSView] = [view]
    while let next = stack.popLast() {
      if let match = next as? NSTextView { found.append(match) }
      stack.append(contentsOf: next.subviews)
    }
    return found
  }
}
