// `TailFollow` (T37.23.7, DT§5.2): the transcript stays at its end while a reply streams, as long
// as the reader left it there. Separate from `TranscriptView` because it is scroll state the
// coordinator keeps, not what the transcript draws; the text view it moves stays unaware of it.

import AppKit
import CoxTranscriptText

/// Keeps a transcript at its end while the reader is there: a patch batch that lands while they
/// are at the bottom scrolls its new end into view; one that lands after they scrolled up leaves
/// the view where it is, until they scroll back to the bottom. Whether to follow is decided when
/// the view scrolls, not when a batch lands, so text that grows below a following view between
/// batches (a card taking its height a turn after it lands) does not end the follow.
@MainActor
final class TailFollow: NSObject {
  private weak var text: TranscriptTextView?
  /// `nil` until the view first scrolls; a batch then asks where the view is.
  private var following: Bool?
  /// Set while a batch lands and the view follows it, so the scroll that causes is not read as
  /// the reader's.
  private var moving = false

  init(_ text: TranscriptTextView) {
    self.text = text
    super.init()
    guard let clip = text.enclosingScrollView?.contentView else { return }
    clip.postsBoundsChangedNotifications = true
    // A selector observer goes with its object, so nothing has to remove it.
    NotificationCenter.default.addObserver(
      self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification, object: clip)
  }

  /// Runs `edit`, then shows the text's end if the reader was following it.
  func around(_ edit: () -> Void) {
    let follow = following ?? atBottom
    moving = true
    defer { moving = false }
    edit()
    guard follow, let text else { return }
    following = true
    text.scrollToEndOfDocument(nil)
  }

  @objc private func scrolled() {
    guard !moving else { return }
    following = atBottom
  }

  /// Whether the view shows the end of its text, within a rounding slack: AppKit rounds a scroll
  /// to the backing's pixels.
  private var atBottom: Bool {
    guard let scroll = text?.enclosingScrollView, let document = scroll.documentView else {
      return false
    }
    return scroll.documentVisibleRect.maxY >= document.bounds.maxY - 1
  }
}
