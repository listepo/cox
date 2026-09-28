// The composer of one open session as observable state (DT§5.3, DT§4.6): the draft, shell
// mode, the files picked from `@` rows, the files attached, the rows the core offers for the
// token being typed, and the earlier prompt ↑ brought back.
// Separate from `SessionStore`, which holds what the core sent back; this holds what the
// person is about to send. It asks the core for rows (`cox_app::Completer`) and sends one
// `Intent`; the command table, the ranking and the files all stay in Rust.

import CoxClient
import Foundation
import Observation
import UniformTypeIdentifiers

@Observable
@MainActor
public final class ComposerStore {
  public private(set) var text = ""
  /// A leading `!` turned the draft into a shell command.
  public private(set) var isShell = false
  /// The shell command's output goes to the agent too (`UserShell{share}`).
  public var shareOutput = true
  /// The `@path` inserts picked from the rows, in the order picked.
  public private(set) var mentions: [String] = []
  /// Files read from what was dropped, pasted or picked; the core decides what reaches the
  /// model (T37.6).
  public private(set) var attachments: [Attachment] = []
  /// Rows for the token being typed; empty when none is offered.
  public private(set) var completions: [Completion] = []
  public private(set) var selection = 0
  /// Why the last send failed; the draft stays so it can be sent again.
  public private(set) var failure: String?
  @ObservationIgnored public let session: SessionStore
  /// The session's earlier prompts, newest first, and which one the draft shows, while ↑ ↓ walk
  /// them; `nil` once the draft is typed, sent or walked back to empty.
  private var recalled: (prompts: [String], index: Int)?

  /// Rows asked for at a time: more than the list shows scrolls nothing into view.
  static let rowLimit: UInt32 = 8
  /// Earlier prompts asked for when ↑ starts a walk.
  static let historyLimit: UInt32 = 100

  public init(session: SessionStore) {
    self.session = session
  }

  /// Something to send.
  public var canSend: Bool {
    !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      || (!isShell && !attachments.isEmpty)
  }

  /// The draft as typed. A `!` typed into an empty draft enters shell mode instead.
  public func edit(_ new: String) {
    if !isShell, text.isEmpty, new == "!" {
      isShell = true
      text = ""
      completions = []
      return
    }
    if new != text { recalled = nil }
    text = new
    mentions.removeAll { !text.contains($0) }
    complete()
  }

  public func moveSelection(by step: Int) {
    guard !completions.isEmpty else { return }
    selection = (selection + step + completions.count) % completions.count
  }

  /// Puts row `index`'s insert in place of the token being typed.
  public func pick(_ index: Int) {
    guard completions.indices.contains(index), let token = typedToken else { return }
    let insert = completions[index].insert
    text = String(text.dropLast(token.count)) + insert + " "
    if insert.hasPrefix("@"), !mentions.contains(insert) { mentions.append(insert) }
    completions = []
  }

  public func dismissCompletion() { completions = [] }

  /// The draft shows an earlier prompt, so ↑ and ↓ keep walking.
  public var isRecalling: Bool { recalled != nil }

  /// ↑ (-1) to an older prompt or ↓ (+1) to a newer one. A walk starts with ↑ in an empty draft
  /// and ends with ↓ past the newest prompt, which empties the draft again.
  public func recall(_ step: Int) {
    if recalled == nil {
      guard step < 0, text.isEmpty, !isShell else { return }
      do {
        recalled = (try session.session.history(limit: Self.historyLimit), -1)
      } catch {
        return report(error)
      }
    }
    guard let (prompts, index) = recalled else { return }
    let next = index - step
    if next < 0 {
      (recalled, text) = (nil, "")
    } else if prompts.indices.contains(next) {
      (recalled, text, completions) = ((prompts, next), prompts[next], [])
    } else if index < 0 {
      recalled = nil
    }
  }

  /// Takes a picked file back out of the draft.
  public func removeMention(_ insert: String) {
    mentions.removeAll { $0 == insert }
    if let range = text.range(of: insert + " ") ?? text.range(of: insert) {
      text.removeSubrange(range)
    }
    complete()
  }

  public func leaveShell() { isShell = false }

  /// Reads each file off the main actor and attaches it, its media type from its extension. A
  /// file that cannot be read is named in `failure`; the others are still attached.
  public func attach(_ urls: [URL]) async {
    for url in urls {
      do {
        let data = try await Task.detached { try Self.read(url) }.value
        let type = UTType(filenameExtension: url.pathExtension)
        attach(data, name: url.lastPathComponent, type: type)
      } catch {
        report(error)
      }
    }
  }

  /// Bytes with no file behind them, such as a pasted screenshot.
  public func attach(_ data: Data, name: String, type: UTType?) {
    attachments.append(
      Attachment(
        name: name, mediaType: type?.preferredMIMEType ?? "application/octet-stream",
        dataB64: data.base64EncodedString()))
  }

  public func removeAttachment(at index: Int) {
    if attachments.indices.contains(index) { attachments.remove(at: index) }
  }

  /// Something the view could not do for the draft, such as open the file picker.
  public func report(_ error: any Error) {
    failure = String(describing: error)
  }

  /// A picked file may be security-scoped (a sandboxed file picker's); reading it asks for
  /// access for as long as the read takes.
  private nonisolated static func read(_ url: URL) throws -> Data {
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    return try Data(contentsOf: url)
  }

  /// A turn runs, as the core's usage view says (`TurnStarted` until `TurnDone`).
  public var isRunning: Bool { session.usage?.turn.map { !$0.done } ?? false }

  /// Prompts queued behind the running turn that have not started: each starts its own turn,
  /// so every turn begun since the first was queued takes one off.
  public var queued: Int {
    guard let queue else { return 0 }
    return max(0, queue.count - Int(latestTurn) + Int(queue.turn))
  }

  /// The turn that ran when the queue began, and how many prompts joined it.
  private var queue: (turn: UInt32, count: Int)?

  private var latestTurn: UInt32 { session.blocks.values.last?.turn ?? 0 }

  /// Sends the draft: a shell line, a `/` command line for the core's command table, or a
  /// turn with the attachments — queued behind the running turn, where `Intent.queue` carries
  /// text only, so a draft with attachments waits for ⌘⏎. The draft clears once the core took
  /// it; attachments stay for a shell or command line, which cannot carry them.
  public func submit() async {
    guard canSend else { return }
    let turn = !isShell && !text.hasPrefix("/")
    guard !(turn && isRunning && !attachments.isEmpty) else {
      failure = "Attachments cannot wait in the queue; ⌘⏎ sends them now."
      return
    }
    await send(turn && isRunning ? .queue(text: text) : draftIntent)
  }

  /// ⌘⏎: interrupts the running turn, then sends the draft as a turn of its own.
  public func submitNow() async {
    guard canSend else { return }
    guard isRunning else { return await submit() }
    do {
      _ = try await session.send(.interrupt)
    } catch {
      return report(error)
    }
    await send(draftIntent)
  }

  private var draftIntent: Intent {
    if isShell { return .shell(command: text, share: shareOutput) }
    return text.hasPrefix("/") ? .command(line: text) : .send(text: text, attachments: attachments)
  }

  private func send(_ intent: Intent) async {
    do {
      _ = try await session.send(intent)
      switch intent {
      case .send: attachments = []
      case .queue:
        let count = queued
        queue = count == 0 ? (latestTurn, 1) : queue.map { ($0.turn, $0.count + 1) }
      default: break
      }
      (text, mentions, completions, isShell, failure) = ("", [], [], false, nil)
      recalled = nil
    } catch {
      report(error)
    }
  }

  /// The word at the end of the draft when it asks for rows: an `@` file anywhere, a `/`
  /// command only as the draft's first word.
  private var typedToken: Substring? {
    guard !isShell, let last = text.last, !last.isWhitespace else { return nil }
    let start = text.lastIndex(where: \.isWhitespace).map { text.index(after: $0) }
    let token = text[(start ?? text.startIndex)...]
    if token.hasPrefix("@") || (token.hasPrefix("/") && start == nil) { return token }
    return nil
  }

  private func complete() {
    completions =
      typedToken.map { session.session.complete(String($0), limit: Self.rowLimit) } ?? []
    selection = 0
  }
}
