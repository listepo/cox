// The terminal pane's bridge over a fake `TerminalClient` (T51.5): keys typed into SwiftTerm's
// view reach `write`, a new frame reaches `resize` in cells, and bytes the shell prints appear
// in the view's buffer. No process is spawned; the fake is the whole shell.

import AppKit
import CoxClient
import SwiftTerm
import Synchronization
import Testing

@testable import CoxPlatform

/// Records what the view sends and hands out what a test yields.
final class FakeTerminal: TerminalClient {
  struct Size: Equatable, Sendable {
    var cols: UInt16
    var rows: UInt16
  }

  let written = Mutex<[UInt8]>([])
  let sizes = Mutex<[Size]>([])
  let outputs: AsyncStream<[UInt8]>
  let shell: AsyncStream<[UInt8]>.Continuation

  init() {
    (outputs, shell) = AsyncStream.makeStream(of: [UInt8].self)
  }

  func write(_ bytes: [UInt8]) throws { written.withLock { $0 += bytes } }
  func resize(cols: UInt16, rows: UInt16) throws {
    sizes.withLock { $0.append(Size(cols: cols, rows: rows)) }
  }
  func exitStatus() -> UInt32? { nil }
  func close() { shell.finish() }
}

@MainActor
private func mounted(_ client: FakeTerminal) -> (TerminalView, TerminalBridge) {
  let view = TerminalView(
    frame: NSRect(x: 0, y: 0, width: 640, height: 320),
    font: .monospacedSystemFont(ofSize: 12, weight: .regular))
  let bridge = TerminalBridge(client: client, openLink: { _ in })
  bridge.attach(view)
  // The view holds its delegate weakly; the caller keeps the bridge.
  return (view, bridge)
}

@MainActor @Test func typedKeysReachTheShellsWrite() {
  let fake = FakeTerminal()
  let (view, bridge) = mounted(fake)
  view.insertText("ls -la\r", replacementRange: NSRange(location: NSNotFound, length: 0))
  #expect(fake.written.withLock { $0 } == Array("ls -la\r".utf8))
  bridge.detach()
}

@MainActor @Test func aNewFrameReachesResizeInCells() {
  let fake = FakeTerminal()
  let (view, bridge) = mounted(fake)
  view.setFrameSize(NSSize(width: 960, height: 540))
  let terminal = view.getTerminal()
  let last = fake.sizes.withLock { $0.last }
  #expect(
    last == FakeTerminal.Size(cols: UInt16(terminal.cols), rows: UInt16(terminal.rows)))
  #expect((last?.cols ?? 0) > 80, "a wider frame is more columns")
  bridge.detach()
}

@MainActor @Test func bytesTheShellPrintsAppearInTheBuffer() async throws {
  let fake = FakeTerminal()
  let (view, bridge) = mounted(fake)
  fake.shell.yield(Array("hello from the shell\r\n".utf8))
  var text = ""
  for _ in 0..<200 where !text.contains("hello from the shell") {
    try await Task.sleep(for: .milliseconds(10))
    text = String(decoding: view.getTerminal().getBufferAsData(), as: UTF8.self)
  }
  #expect(text.contains("hello from the shell"))
  bridge.detach()
}

/// What the caller's `openLink` was handed.
@MainActor
private final class Links {
  var opened: [String] = []
}

@MainActor @Test func aClickedLinkGoesToTheCallerNotTheWorkspace() {
  let fake = FakeTerminal()
  let links = Links()
  let view = TerminalView(frame: NSRect(x: 0, y: 0, width: 640, height: 320), font: nil)
  let bridge = TerminalBridge(client: fake, openLink: { links.opened.append($0) })
  bridge.attach(view)
  bridge.requestOpenLink(source: view, link: "file:///etc/passwd", params: [:])
  #expect(links.opened == ["file:///etc/passwd"])
  #expect(bridge.clipboardRead(source: view) == nil)
  bridge.detach()
}
