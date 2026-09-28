// HostBridge forwards Rust's host calls to a `PlatformHost` (DT§4.4): the
// secret it answers, the URL, and an inbox item as a `HostNote`. The
// platform host here records calls; it holds no Keychain (A49).

import CoxClient
import CoxFFIBindings
import Synchronization
import Testing

@testable import CoxCore

final class RecordingHost: PlatformHost {
  let notes = Mutex<[HostNote]>([])
  let opened = Mutex<[String]>([])

  func secret(for section: String) -> String? { section == "anthropic" ? "sk-ant" : nil }
  func notify(_ note: HostNote) { notes.withLock { $0.append(note) } }
  func open(_ url: String) { opened.withLock { $0.append(url) } }
}

@Test func theBridgeAnswersThePlatformHostsSecret() {
  let bridge = HostBridge(RecordingHost())
  #expect(bridge.secret(section: "anthropic") == "sk-ant")
  #expect(bridge.secret(section: "openai") == nil)
}

@Test func anInboxItemBecomesANoteForItsSession() {
  let host = RecordingHost()
  let bridge = HostBridge(host)
  let question = Need.question(callId: "c1", question: "Which branch?", options: [])
  bridge.notify(
    item: InboxItem(session: "s1", source: nil, need: question, expired: false, seq: 1), badge: 2)
  let done = Need.taskDone(task: "t1", label: "tests", ok: false)
  bridge.notify(
    item: InboxItem(session: "s2", source: nil, need: done, expired: false, seq: 2), badge: 0)
  bridge.openUrl(url: "https://example.com")
  #expect(
    host.notes.withLock { $0 } == [
      HostNote(session: "s1", kind: .question, text: "Which branch?", badge: 2),
      HostNote(session: "s2", kind: .taskDone(succeeded: false), text: "tests", badge: 0),
    ])
  #expect(host.opened.withLock { $0 } == ["https://example.com"])
}
