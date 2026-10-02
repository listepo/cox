// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// A hovered prompt's actions (T37.23.9, DT§5.2): while the pointer is over a prompt, the view
// `TranscriptCards.promptActions` makes for it floats over its bubble's top trailing corner.
// Its own file because it is the one place the transcript follows the pointer; the actions and
// what they do are the caller's.

import AppKit
import CoxClient
import SwiftUI

extension TranscriptTextView {
  /// Marks the tracking area that follows the pointer for prompts, apart from AppKit's own.
  private static let promptTracking = "cox.transcript.prompts"

  func trackPrompts() {
    let options: NSTrackingArea.Options = [
      .mouseMoved, .mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect,
    ]
    addTrackingArea(
      NSTrackingArea(
        rect: .zero, options: options, owner: self, userInfo: [Self.promptTracking: true]))
  }

  override public func mouseMoved(with event: NSEvent) {
    super.mouseMoved(with: event)
    hover(at: convert(event.locationInWindow, from: nil))
  }

  override public func mouseExited(with event: NSEvent) {
    super.mouseExited(with: event)
    if event.trackingArea?.userInfo?[Self.promptTracking] != nil { hover(at: nil) }
  }

  /// The prompt whose actions show, if any.
  public var hoveredPrompt: BlockID? { promptHover?.id }

  /// Shows the actions of the prompt under `point`, in this view's coordinates; `nil`, or a
  /// point off every prompt, hides them. A point on the actions keeps them.
  public func hover(at point: NSPoint?) {
    if let point, let shown = promptHover, shown.view.frame.contains(point) { return }
    let id = point.flatMap(prompt(at:))
    guard id != promptHover?.id else { return }
    promptHover?.view.removeFromSuperview()
    promptHover = nil
    guard let id, let block = blocks[id], let make = cards.promptActions, let top = bubbleTop(id)
    else { return }
    let host = NSHostingView(rootView: make(block))
    let size = host.fittingSize
    // Inside the bubble's top trailing corner, in by its own padding: above or across its edge
    // the first prompt's strip would be cut off by the top of the view.
    let padding = style.bubble.padding
    host.frame = NSRect(
      origin: NSPoint(x: top.maxX - padding.width - size.width, y: top.minY + padding.height),
      size: size)
    addSubview(host)
    promptHover = (id, host)
  }

  /// The prompt block laid out at `point`, the space after it included.
  private func prompt(at point: NSPoint) -> BlockID? {
    guard let manager = textLayoutManager, let content = manager.textContentManager else {
      return nil
    }
    let origin = textContainerOrigin
    let inContainer = CGPoint(x: point.x - origin.x, y: point.y - origin.y)
    guard let fragment = manager.textLayoutFragment(for: inContainer) else { return nil }
    let offset = content.offset(
      from: content.documentRange.location, to: fragment.rangeInElement.location)
    guard let id = blockID(at: offset), case .user = blocks[id]?.kind else { return nil }
    return id
  }

  /// The top slice of prompt `id`'s bubble in this view's coordinates, as its fragment draws it.
  private func bubbleTop(_ id: BlockID) -> CGRect? {
    guard let range = range(of: id), range.length > 0,
      let location = textRange(range)?.location,
      let fragment = textLayoutManager?.textLayoutFragment(for: location) as? DecorFragment,
      let (decor, edge) = fragment.decoration, decor.kind == .bubble,
      let area = decor.area(fragment, edge)
    else { return nil }
    let frame = fragment.layoutFragmentFrame
    return area.offsetBy(
      dx: frame.minX + textContainerOrigin.x, dy: frame.minY + textContainerOrigin.y)
  }
}
