// The composer of one open session as observable state (DT§5.3, DT§4.6): the draft, shell
// mode, the files picked from `@` rows, and the rows the core offers for the token being typed.
// Separate from `SessionStore`, which holds what the core sent back; this holds what the
// person is about to send. It asks the core for rows (`cox_app::Completer`) and sends one
// `Intent`; the command table, the ranking and the files all stay in Rust.

import CoxClient
import Foundation
import Observation

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
  /// Rows for the token being typed; empty when none is offered.
  public private(set) var completions: [Completion] = []
  public private(set) var selection = 0
  /// Why the last send failed; the draft stays so it can be sent again.
  public private(set) var failure: String?
  @ObservationIgnored public let session: SessionStore

  /// Rows asked for at a time: more than the list shows scrolls nothing into view.
  static let rowLimit: UInt32 = 8

  public init(session: SessionStore) {
    self.session = session
  }

  /// Something to send.
  public var canSend: Bool { !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

  /// The draft as typed. A `!` typed into an empty draft enters shell mode instead.
  public func edit(_ new: String) {
    if !isShell, text.isEmpty, new == "!" {
      isShell = true
      text = ""
      completions = []
      return
    }
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

  /// Takes a picked file back out of the draft.
  public func removeMention(_ insert: String) {
    mentions.removeAll { $0 == insert }
    if let range = text.range(of: insert + " ") ?? text.range(of: insert) {
      text.removeSubrange(range)
    }
    complete()
  }

  public func leaveShell() { isShell = false }

  /// Sends the draft: a shell line, a `/` command line for the core's command table, or a
  /// turn. The draft clears once the core took it.
  public func submit() async {
    guard canSend else { return }
    let intent: Intent =
      isShell
      ? .shell(command: text, share: shareOutput)
      : text.hasPrefix("/") ? .command(line: text) : .send(text: text, attachments: [])
    do {
      _ = try await session.send(intent)
      (text, mentions, completions, isShell, failure) = ("", [], [], false, nil)
    } catch {
      failure = String(describing: error)
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
