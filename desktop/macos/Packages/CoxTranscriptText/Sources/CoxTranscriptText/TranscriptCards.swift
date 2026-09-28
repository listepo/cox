// Cards in the transcript text (T37.41, DT§5.2): a tool, approval, question or
// subagent block is one attachment character hosting a SwiftUI view, so a drag selects
// it as one unit and the text around it stays one string (research.md
// §9.5.13). Its own file because it is the one place SwiftUI meets TextKit:
// the card views come from the caller (CoxUI, T37.23), this package only
// hosts, sizes and re-lays them out.

import AppKit
import CoxClient
import SwiftUI

/// The SwiftUI view each card block shows, and the small views inside a prompt
/// and a thought (T37.23.4): an attachment's tile and a thought's fold header,
/// given its title (`thoughtTitle`), whether it is open and what opens or folds it. CoxUI supplies its
/// catalogue views; `summary` draws plain text for tests and previews.
public struct TranscriptCards {
  let view: @MainActor (Block) -> AnyView
  let thumbnail: @MainActor (String) -> AnyView
  let header:
    @MainActor (_ title: String, _ open: Bool, _ toggle: @escaping @MainActor () -> Void) ->
      AnyView
  /// Set by the view that hosts the cards: a card's view was made (`false`)
  /// or its height changed (`true`).
  var changed: @MainActor (BlockID, _ resized: Bool) -> Void = { _, _ in }
  /// Set by the view that hosts the cards: whether a thought is open, and its fold toggled.
  var isOpen: @MainActor (BlockID) -> Bool = { _ in false }
  var toggle: @MainActor (BlockID) -> Void = { _ in }
  /// The actions a hovered prompt shows over its bubble (`PromptHover.swift`); `nil` shows none.
  public var promptActions: (@MainActor (Block) -> AnyView)?

  public init<Card: View, Tile: View, Header: View>(
    _ view: @escaping @MainActor (Block) -> Card,
    thumbnail: @escaping @MainActor (String) -> Tile,
    thinking header: @escaping @MainActor (String, Bool, @escaping @MainActor () -> Void) -> Header
  ) {
    self.view = { AnyView(view($0)) }
    self.thumbnail = { AnyView(thumbnail($0)) }
    self.header = { AnyView(header($0, $1, $2)) }
  }

  public init<Card: View>(_ view: @escaping @MainActor (Block) -> Card) {
    self.init(
      view, thumbnail: { Text($0) },
      thinking: { title, open, toggle in Button(open ? "Fold" : title) { toggle() } })
  }

  /// A thought's fold header (DS§6.3): "Thinking" while it streams, then how
  /// long the model thought, "Thought for 12 s", in whole seconds and never 0.
  static func thoughtTitle(_ kind: BlockKind) -> String {
    guard case .thinking(_, let durationMs?) = kind else { return "Thinking" }
    return "Thought for \(max(1, (durationMs + 500) / 1000)) s"
  }

  public static var summary: TranscriptCards {
    TranscriptCards { Text(MarkdownCopy.card($0.kind) ?? "") }
  }

  /// A block with a summary line is drawn as a card, not as text.
  static func isCard(_ kind: BlockKind) -> Bool { MarkdownCopy.card(kind) != nil }
}

/// One card: the attachment character's view, made when TextKit first draws
/// its line so a long transcript builds no views it never shows, and kept for
/// the block's life so the card's own state (expanded or not) survives
/// re-layout.
///
/// TextKit 2 asks for the bounds and the view on the main thread, while the
/// text view lays out, hence `assumeIsolated` in the nonisolated overrides.
@MainActor
final class CardAttachment: NSTextAttachment {
  /// A card fills its line; a prompt's tile and a thought's header take their own size.
  enum Role: Equatable {
    case card, header
    case thumbnail(String)
  }

  private(set) var block: Block
  private let cards: TranscriptCards
  private let role: Role
  private var host: CardHost?

  init(_ block: Block, cards: TranscriptCards, role: Role = .card) {
    (self.block, self.cards, self.role) = (block, cards, role)
    super.init(data: nil, ofType: nil)
    // An empty image, not none: with none TextKit draws its document placeholder under the
    // view, and it shows through a card with no background of its own. (Overriding
    // `image(for:)` instead turns the view provider off.)
    image = NSImage()
  }

  nonisolated required init?(coder: NSCoder) { nil }

  /// Shows `block`'s new state in the same view, so SwiftUI keeps the card's
  /// own state; a new height re-lays the card out as any resize does.
  func update(_ block: Block) {
    self.block = block
    host?.rootView.card = view(block)
  }

  private func view(_ block: Block) -> AnyView {
    switch role {
    case .card: return cards.view(block)
    case .thumbnail(let name): return cards.thumbnail(name)
    case .header:
      let (id, cards) = (block.id, cards)
      let title = TranscriptCards.thoughtTitle(block.kind)
      return cards.header(title, cards.isOpen(id)) { cards.toggle(id) }
    }
  }

  var hostView: CardHost {
    if let host { return host }
    let made = CardHost(rootView: CardFrame(card: view(block), width: role == .card ? 0 : nil))
    made.sizingOptions = [.intrinsicContentSize]
    made.onResize = { [weak self] in self.map { $0.cards.changed($0.block.id, true) } }
    host = made
    // TextKit makes the view while it draws the line and places views only
    // when it lays lines out, so the viewport is laid out once more.
    let id = block.id
    nextTurn { [cards] in cards.changed(id, false) }
    return made
  }

  nonisolated override func viewProvider(
    for parentView: NSView?, location: any NSTextLocation, textContainer: NSTextContainer?
  ) -> NSTextAttachmentViewProvider? {
    let provider = CardViewProvider(
      textAttachment: self, parentView: parentView,
      textLayoutManager: textContainer?.textLayoutManager, location: location)
    // The bounds are ours (`attachmentBounds`), not the view's.
    provider.tracksTextAttachmentViewBounds = false
    return provider
  }

  /// A card: the full line width, as tall as the card is at that width. A tile
  /// or a header: its own size. The bottom sits on the descender so the line is
  /// no taller than the view.
  nonisolated override func attachmentBounds(
    for attributes: [NSAttributedString.Key: Any], location: any NSTextLocation,
    textContainer: NSTextContainer?, proposedLineFragment: CGRect, position: CGPoint
  ) -> CGRect {
    let padding = textContainer?.lineFragmentPadding ?? 0
    let width = max(0, proposedLineFragment.width - 2 * padding)
    let descender = (attributes[.font] as? NSFont)?.descender ?? 0
    nonisolated(unsafe) let attachment = self
    let size = MainActor.assumeIsolated {
      attachment.hostView.size(width: attachment.role == .card ? width : nil)
    }
    return CGRect(x: 0, y: descender, width: size.width, height: size.height)
  }
}

final class CardViewProvider: NSTextAttachmentViewProvider {
  override func loadView() {
    // Main thread, as for `CardAttachment`'s overrides.
    nonisolated(unsafe) let provider = self
    MainActor.assumeIsolated {
      provider.view = (provider.textAttachment as? CardAttachment)?.hostView
    }
  }
}

/// The card at the width TextKit gives it (`nil`: its own width): SwiftUI picks the height.
struct CardFrame: View {
  var card: AnyView
  var width: CGFloat?

  var body: some View {
    card.frame(width: width, alignment: .leading)
      .fixedSize(horizontal: width == nil, vertical: true)
  }
}

/// Tells the transcript when the card's height changes, as when it expands.
/// SwiftUI invalidates the intrinsic size when a card grows but only lays out
/// again when it shrinks, so both paths check.
final class CardHost: NSHostingView<CardFrame> {
  var onResize: (() -> Void)?
  private var measured: CGFloat?
  private var pending = false

  func size(width: CGFloat?) -> NSSize {
    if rootView.width != width { rootView.width = width }
    let size = intrinsicContentSize
    measured = size.height
    return NSSize(width: width ?? size.width, height: size.height)
  }

  override func invalidateIntrinsicContentSize() {
    super.invalidateIntrinsicContentSize()
    checkHeight()
  }

  override func layout() {
    super.layout()
    checkHeight()
  }

  private func checkHeight() {
    guard let measured, !pending, intrinsicContentSize.height != measured else { return }
    // Not inside TextKit's layout pass that asked for the size.
    pending = true
    nextTurn { [weak self] in
      self?.pending = false
      self?.onResize?()
    }
  }
}

/// Runs `body` on the main run loop's next turn, after the layout or drawing
/// pass that asked for it. The run loop rather than the main queue, so a
/// nested run loop (a modal session, a test waiting on the view) turns it too.
@MainActor
func nextTurn(_ body: @escaping @MainActor () -> Void) {
  RunLoop.main.perform { MainActor.assumeIsolated { body() } }
}

extension TranscriptTextView {
  /// `cards`, reporting to this view.
  var hostedCards: TranscriptCards {
    var hosted = cards
    hosted.changed = { [weak self] in self?.cardChanged($0, resized: $1) }
    hosted.isOpen = { [weak self] in self?.openThoughts.contains($0) ?? false }
    // After the click that asked: the fold edits the text the header sits in.
    hosted.toggle = { [weak self] id in
      nextTurn { self.map { $0.setThought(id, open: !$0.openThoughts.contains(id)) } }
    }
    return hosted
  }

  /// Lays the viewport out again, and a resized card's one character with
  /// it, so the text below moves with the card while its ranges stay put.
  func cardChanged(_ id: BlockID, resized: Bool) {
    // Out of a window the viewport is the whole text: nothing to place.
    guard window != nil, let manager = textLayoutManager else { return }
    if resized, let range = range(of: id), let text = textRange(range) {
      manager.invalidateLayout(for: text)
    }
    manager.textViewportLayoutController.layoutViewport()
  }

  /// `range` as TextKit 2 locations.
  func textRange(_ range: NSRange) -> NSTextRange? {
    guard let content = textLayoutManager?.textContentManager,
      let start = content.location(content.documentRange.location, offsetBy: range.location),
      let end = content.location(start, offsetBy: range.length)
    else { return nil }
    return NSTextRange(location: start, end: end)
  }
}
