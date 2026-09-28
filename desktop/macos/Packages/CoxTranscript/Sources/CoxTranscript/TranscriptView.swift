// `TranscriptView` (DS§6.4 row `TranscriptView`, DT§5.2): a session's timeline as one
// selectable document — `CoxTranscriptText`'s TextKit 2 view with the blocks as text and the
// card blocks as CoxUI cards — kept in step with the session by the same patches its
// `SessionStore` applies, so a streamed reply edits only its own range. Separate as the one
// SwiftUI view that hosts the AppKit transcript; what it draws comes from `TranscriptCard` and
// `TranscriptStyle.cox`.

import AppKit
import CoxClient
import CoxModel
import CoxTranscriptText
import CoxUI
import SwiftUI

/// The transcript of `store`'s session. `crossBlockSelection` is the app's
/// `[desktop.transcript] cross_block_selection` (A67): on, a drag runs across blocks like a
/// document; off, it stays in the block it started in. Approvals and questions show in the
/// `approval` slot.
public struct TranscriptView<Approval: View>: NSViewRepresentable {
  let store: SessionStore
  let crossBlockSelection: Bool
  let approval: @MainActor (Block) -> Approval

  public init(
    store: SessionStore, crossBlockSelection: Bool = true,
    @ViewBuilder approval: @escaping @MainActor (Block) -> Approval
  ) {
    self.store = store
    self.crossBlockSelection = crossBlockSelection
    self.approval = approval
  }

  public func makeCoordinator() -> TranscriptCoordinator { TranscriptCoordinator() }

  public func makeNSView(context: Context) -> NSScrollView {
    let appearance = context.environment.coxAppearance
    let shared = context.coordinator.appearance
    shared.value = appearance
    shared.locale = context.environment.locale
    let text = TranscriptTextView.make(style: .cox(textScale: appearance.textScale))
    text.cards = TranscriptCards { [approval] block in
      CardAppearance(shared: shared) { TranscriptCard(block: block, approval: approval) }
    } thumbnail: { name in
      CardAppearance(shared: shared) { Thumbnail(name) }
    } thinking: { open, toggle in
      CardAppearance(shared: shared) {
        ThinkingHeader("Thinking", isExpanded: open, action: toggle)
      }
    }
    text.crossBlockSelection = crossBlockSelection
    text.drawsBackground = false
    let scroll = text.inScrollView(frame: .zero)
    scroll.drawsBackground = false
    text.load(store.blocks.values)
    context.coordinator.follow(store, into: text)
    return scroll
  }

  public func updateNSView(_ scroll: NSScrollView, context: Context) {
    (scroll.documentView as? TranscriptTextView)?.crossBlockSelection = crossBlockSelection
    context.coordinator.appearance.value = context.environment.coxAppearance
    context.coordinator.appearance.locale = context.environment.locale
  }

  public static func dismantleNSView(_ scroll: NSScrollView, coordinator: TranscriptCoordinator) {
    coordinator.stop()
  }
}

/// What the transcript keeps across SwiftUI updates: the store it follows and the appearance
/// its cards read, which reaches them here because a hosted card is not in SwiftUI's tree.
@MainActor
public final class TranscriptCoordinator {
  let appearance = SharedAppearance()
  private weak var store: SessionStore?
  private var tail: TailFollow?

  /// Splices each batch the store applies into `text`, after the store, so a block `current`
  /// returns is as the batch left it, keeping the view at the end while the reader is there
  /// (`TailFollow`).
  func follow(_ store: SessionStore, into text: TranscriptTextView) {
    self.store = store
    let tail = TailFollow(text)
    self.tail = tail
    store.didApply = { [weak text, weak store] patches in
      tail.around { text?.apply(patches) { store?.blocks[$0] } }
    }
  }

  func stop() { store?.didApply = nil }
}

/// The `coxAppearance` and locale the transcript was given, observed by every card it hosts.
@Observable
@MainActor
final class SharedAppearance {
  var value = Appearance()
  var locale = Locale.current
}

/// A hosted card with the transcript's appearance and locale.
struct CardAppearance<Content: View>: View {
  let shared: SharedAppearance
  @ViewBuilder let content: Content

  var body: some View {
    content.environment(\.coxAppearance, shared.value).environment(\.locale, shared.locale)
  }
}

extension TranscriptStyle {
  /// The transcript drawn with CoxUI's tokens: `font.transcript` prose and `font.mono.code`
  /// code at the user's text size, readable colours only (DS§8) — the status colours miss
  /// 4.5:1 as text, so `ok`, `warn`, `error` and the diff tokens keep `text.primary`. A prompt
  /// sits on `UserBubble`'s face and a thought reads as `ThinkingDisclosure` (T37.21.5).
  @MainActor
  static func cox(textScale: Double) -> TranscriptStyle {
    let secondary = TextColour.secondary.nsColor
    return TranscriptStyle(
      body: FontToken.transcript.nsFont(scale: textScale),
      code: FontToken.monoCode.nsFont(scale: textScale),
      text: TextColour.primary.nsColor,
      colors: [
        .dim: secondary, .tool: secondary, .diffHunk: secondary,
        .accent: TextColour.accent.nsColor, .border: TextColour.tertiary.nsColor,
      ],
      blockSpacing: Space.l, inset: NSSize(width: Space.xl, height: Space.xl),
      bubble: .init(
        fill: SurfaceColour.fillPrimary.nsColor, radius: Radius.xl,
        padding: NSSize(width: Space.l, height: Space.ml), gap: Space.m),
      thought: .init(
        font: NSFontManager.shared.convert(
          FontToken.caption.nsFont(scale: textScale), toHaveTrait: .italicFontMask),
        color: secondary, rule: SurfaceColour.separator.nsColor, ruleWidth: Size.hairline,
        indent: Space.l))
  }
}
