// `Composer` (DS§6.4 row `Composer`, the mockup's `.composer`; DT§5.3; mockup screens 1, 5–7):
// where the next message is written — the text, the files it mentions, shell mode with its
// "share output" switch, the prompts queued behind the running turn, the completion rows for
// `@` and `/`, and Send. Separate so the transcript column shows it from one value and reports
// every key and click as an intent; the store behind it decides what each one sends.

import SwiftUI

/// The editor over a row of chips and Send, on readable window glass at e3 — the one thing that
/// floats highest in a pane (DS§3.4). The completion rows float above it. ⏎ sends (or picks the
/// selected row while rows show), ⇧⏎ breaks the line, ⌘⏎ sends now, ↑ ↓ ⇥ and ⎋ drive the
/// rows, and ⌫ in an empty shell line leaves shell mode. It holds no draft of its own.
public struct Composer: View {
  public struct State: Equatable, Sendable {
    public var text = ""
    /// A leading `!` turned the line into a shell command (DT§5.3).
    public var isShell = false
    /// The shell command's output goes to the agent too (`UserShell{share}`).
    public var shareOutput = true
    /// Files picked from the `@` rows.
    public var mentions: [Mention] = []
    /// Rows for the token being typed, or `nil`.
    public var completion: CompletionList.State?
    /// A turn runs, so ⏎ queues the message.
    public var isRunning = false
    /// Prompts queued behind the running turn.
    public var queued = 0
    /// Something to send: text or an attachment.
    public var canSend = false

    public init() {}
  }

  /// A mentioned file: its `@path`, as the core's completion inserted it, and its label.
  public struct Mention: Equatable, Sendable, Identifiable {
    public var id: String
    public var label: String

    public init(id: String, label: String) {
      self.id = id
      self.label = label
    }
  }

  public enum Intent: Equatable, Sendable {
    /// The text as typed.
    case edit(String)
    /// ⏎ or Send: sends, or queues while a turn runs.
    case submit
    /// ⌘⏎: interrupts the running turn and sends.
    case submitNow
    /// ↑ (-1) or ↓ (+1) through the completion rows.
    case moveSelection(Int)
    /// A completion row, by index.
    case pick(Int)
    /// ⎋ while the rows show.
    case dismissCompletion
    case removeMention(String)
    case leaveShell
    case shareOutput(Bool)
  }

  let state: State
  let send: (Intent) -> Void

  public init(state: State, send: @escaping (Intent) -> Void) {
    self.state = state
    self.send = send
  }

  public var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.pane, style: .continuous)
    VStack(alignment: .leading, spacing: 0) {
      ComposerEditor(state: state, send: send)
        .padding(.horizontal, Space.l)
        .padding(.top, Space.l)
        .padding(.bottom, Space.xs)
      ComposerChipRow(state: state, send: send)
        .padding(.horizontal, Space.ml)
        .padding(.bottom, Space.ml)
    }
    .frame(maxWidth: Size.readingWidth)
    .glassPane(shape, surface: Color(.surfaceWindow), role: .readable)
    .hairline(in: shape)
    .elevation(.e3, cornerRadius: Radius.pane)
    .overlay(alignment: .topLeading) {
      // A line along the composer's top edge that the rows stand on, so they grow upwards.
      Color.clear.frame(height: 0).overlay(alignment: .bottomLeading) {
        if let completion = state.completion {
          CompletionList(state: completion) { send(.pick($0)) }
            .fixedSize()
            .padding(.leading, Space.l)
            .padding(.bottom, Space.m)
        }
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Composer")
  }
}

/// The text, in `font.transcript` — or `font.mono.code` for a shell line — growing with what is
/// typed up to `maxHeight`, then scrolling; the hint shows while it is empty.
private struct ComposerEditor: View {
  let state: Composer.State
  let send: (Composer.Intent) -> Void

  /// About ten lines of `font.transcript`; a longer message scrolls inside the editor.
  private static let maxHeight: CGFloat = 220

  var body: some View {
    let font: FontToken = state.isShell ? .monoCode : .transcript
    ZStack(alignment: .topLeading) {
      // Sizes the editor to its text: a `TextEditor` takes all the height it is offered.
      Text(state.text + " ")
        .textStyle(font)
        .fixedSize(horizontal: false, vertical: true)
        .padding(.horizontal, Space.xs)
        .hidden()
        .accessibilityHidden(true)
      if state.text.isEmpty {
        Text(state.isShell ? "Shell command" : "Ask cox…  @ files  / commands  ! shell")
          .textStyle(font)
          .foregroundStyle(Color(.textTertiary))
          .padding(.horizontal, Space.xs)
          .allowsHitTesting(false)
          .accessibilityHidden(true)
      }
      TextEditor(text: Binding(get: { state.text }, set: { send(.edit($0)) }))
        .textStyle(font)
        .foregroundStyle(Color(.textPrimary))
        .scrollContentBackground(.hidden)
        .accessibilityLabel(state.isShell ? "Shell command" : "Message")
        .onKeyPress(action: key)
    }
    .frame(maxHeight: Self.maxHeight)
  }

  /// The keys the composer answers before the text does; any other key types.
  private func key(_ press: KeyPress) -> KeyPress.Result {
    let rows = state.completion != nil
    switch press.key {
    case .return where press.modifiers.contains(.shift):
      return .ignored
    case .return where press.modifiers.contains(.command):
      send(.submitNow)
    case .return:
      send(rows ? .pick(state.completion?.selection ?? 0) : .submit)
    case .tab where rows:
      send(.pick(state.completion?.selection ?? 0))
    case .upArrow where rows:
      send(.moveSelection(-1))
    case .downArrow where rows:
      send(.moveSelection(1))
    case .escape where rows:
      send(.dismissCompletion)
    case .delete where state.isShell && state.text.isEmpty:
      send(.leaveShell)
    default:
      return .ignored
    }
    return .handled
  }
}

/// Shell mode and its switch, the mentioned files, the queue count and Send.
private struct ComposerChipRow: View {
  let state: Composer.State
  let send: (Composer.Intent) -> Void

  var body: some View {
    HStack(spacing: Space.s) {
      if state.isShell {
        ComposerChip("Shell", kind: .shell) { send(.leaveShell) }
        Toggle(
          "Share output", isOn: Binding(get: { state.shareOutput }, set: { send(.shareOutput($0)) })
        )
        .toggleStyle(CoxToggleStyle())
      }
      ForEach(state.mentions) { mention in
        ComposerChip(mention.label, kind: .mention) { send(.removeMention(mention.id)) }
      }
      Spacer(minLength: Space.m)
      if state.queued > 0 {
        ComposerChip("Queued · \(state.queued)", kind: .queued)
      }
      Button {
        send(.submit)
      } label: {
        Image(systemName: "arrow.up").symbolStyle(.body)
      }
      .buttonStyle(CoxButtonStyle(.primary))
      .disabled(!state.canSend)
      .help(state.isRunning ? "Queue after this turn (⏎) · send now (⌘⏎)" : "Send (⏎)")
      .accessibilityLabel(state.isRunning ? "Queue" : "Send")
    }
  }
}

#Preview("empty") { PreviewMatrix { ComposerSample(state: PreviewState.composerEmpty) } }
#Preview("mention") { PreviewMatrix { ComposerSample(state: PreviewState.composerMention) } }
#Preview("commands") { PreviewMatrix { ComposerSample(state: PreviewState.composerCommands) } }
#Preview("shell, queued") { PreviewMatrix { ComposerSample(state: PreviewState.composerShell) } }
