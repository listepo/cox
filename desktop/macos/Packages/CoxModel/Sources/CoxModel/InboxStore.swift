// The sidebar's "Needs you" section (T37.27.7, DT§4.3 Inbox, DS§6.4 `Sidebar`): the app inbox as
// one row per item, an expired one read-only. Here, not in CoxUI, because these decide what the
// section shows (DS§1); the app copies each row into CoxUI's `Sidebar.Session` field for field
// and re-reads the inbox whenever the host hears of a new item or a lower badge.

import CoxClient
import Observation

public struct InboxRow: Identifiable, Equatable, Sendable {
  /// The sidebar's status glyph, named as CoxUI's `StatusDot.Status`.
  public enum Status: Equatable, Sendable { case waiting, idle, error }

  /// The item's own id, as one session can wait on several.
  public let id: String
  /// The session a click opens.
  public let session: String
  public let status: Status
  /// The tool and its subject, the question, the error or the task label.
  public let title: String
  /// What it waits for, after the subagent that asked.
  public let subtitle: String
  /// The session closed or moved to another process: shown, not answerable from here.
  public let isReadOnly: Bool
}

extension InboxRow {
  public init(_ item: InboxItem) {
    var (status, wait): (Status, String) =
      switch item.need {
      case .approval: (.waiting, "approval waiting")
      case .question: (.waiting, "question waiting")
      case .failed: (.error, "turn failed")
      case .taskDone(_, _, true): (.idle, "task done")
      case .taskDone(_, _, false): (.error, "task failed")
      }
    if item.expired { (status, wait) = (.idle, "expired") }
    id = "\(item.session)#\(item.seq)"
    session = item.session
    self.status = status
    title = HostNote(item, badge: 0).text
    subtitle = [item.source?.agent, wait].compactMap { $0 }.joined(separator: " · ")
    isReadOnly = item.expired
  }
}

@Observable
@MainActor
public final class InboxStore {
  /// In the core's order: most urgent first, oldest first within a rank.
  public private(set) var rows: [InboxRow] = []
  /// The same items as the core sent them, for the menu bar's Allow and Deny (T51.14).
  public private(set) var items: [InboxItem] = []
  @ObservationIgnored private let client: any InboxClient

  public init(client: any InboxClient) { self.client = client }

  /// Reads the inbox again; the app calls it on each `PlatformHost.notify` and `badge`.
  public func refresh() {
    items = client.inbox()
    rows = items.map(InboxRow.init)
  }

  /// The section header's count, `nil` when nothing waits.
  public var count: String? { rows.isEmpty ? nil : "\(rows.count)" }
}
