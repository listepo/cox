// Actionable notifications (DT§5.6, T37.27): an approval posts with Allow and Deny, a question
// with a typed answer, and an action on either comes back as the session and the `Intent` to
// send it. Separate from `MacHost` so the note-to-content and action-to-intent mappings are
// plain functions a test checks without posting a notification; `NotificationResponder` is
// the thin delegate the app installs, and the only part that needs the notification centre.

import CoxClient
import Foundation
import UserNotifications

/// Where a notification action goes: the session and the intent to send it.
public struct NotificationRoute: Equatable, Sendable {
  public var session: String
  public var intent: Intent

  public init(session: String, intent: Intent) {
    self.session = session
    self.intent = intent
  }
}

public enum NotificationActions {
  static let approval = "cox.approval"
  static let question = "cox.question"
  static let allow = "cox.allow"
  static let deny = "cox.deny"
  static let answer = "cox.answer"
  static let sessionKey = "session"
  static let callKey = "call"

  /// Allow and Deny on an approval — allow for session and edit need the app, by design
  /// (DT§5.6) — and a typed answer on a question. Allow asks for the unlocked Mac: it runs a
  /// command.
  public static var categories: Set<UNNotificationCategory> {
    let allow = UNNotificationAction(
      identifier: Self.allow, title: "Allow", options: [.authenticationRequired])
    let deny = UNNotificationAction(identifier: Self.deny, title: "Deny", options: [])
    let answer = UNTextInputNotificationAction(
      identifier: Self.answer, title: "Answer", options: [.authenticationRequired],
      textInputButtonTitle: "Send", textInputPlaceholder: "Your answer")
    return [
      UNNotificationCategory(
        identifier: approval, actions: [allow, deny], intentIdentifiers: [], options: []),
      UNNotificationCategory(
        identifier: question, actions: [answer], intentIdentifiers: [], options: []),
    ]
  }

  /// What `note` posts: grouped by session; an approval or question that can still be answered
  /// carries its category and the ids an action needs.
  static func content(for note: HostNote) -> UNMutableNotificationContent {
    let content = UNMutableNotificationContent()
    content.title = title(note.kind)
    content.body = note.text
    content.threadIdentifier = note.session
    content.userInfo[sessionKey] = note.session
    if let call = note.call {
      content.userInfo[callKey] = call
      switch note.kind {
      case .approval: content.categoryIdentifier = approval
      case .question: content.categoryIdentifier = question
      case .failed, .taskDone: break
      }
    }
    return content
  }

  /// The intent an action sends; `nil` for a plain click, an unknown action or an empty answer.
  public static func route(
    action: String, userInfo: [AnyHashable: Any], text: String?
  ) -> NotificationRoute? {
    guard let session = userInfo[sessionKey] as? String, let call = userInfo[callKey] as? String
    else { return nil }
    let intent: Intent
    switch action {
    case allow: intent = .approve(call: call, decision: .allow)
    case deny: intent = .approve(call: call, decision: .deniedByUser)
    case answer:
      guard let text = text?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty
      else { return nil }
      intent = .answer(question: call, text: text)
    default: return nil
    }
    return NotificationRoute(session: session, intent: intent)
  }

  static func title(_ kind: HostNote.Kind) -> String {
    switch kind {
    case .approval: "Approval needed"
    case .question: "Question"
    case .failed: "Turn failed"
    case .taskDone(let succeeded): succeeded ? "Task done" : "Task failed"
    }
  }
}

/// The notification centre's delegate: an action on a cox notification goes to `handle`, which
/// the app points at the session's store, and a click on the notification itself goes to `show`
/// with its session. The app keeps it alive and sets it as
/// `UNUserNotificationCenter.current().delegate` at launch.
public final class NotificationResponder: NSObject, UNUserNotificationCenterDelegate, Sendable {
  private let handle: @Sendable (NotificationRoute) -> Void
  private let show: @Sendable (String) -> Void

  public init(
    handle: @escaping @Sendable (NotificationRoute) -> Void,
    show: @escaping @Sendable (String) -> Void = { _ in }
  ) {
    (self.handle, self.show) = (handle, show)
  }

  public func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
  ) async {
    let text = (response as? UNTextInputNotificationResponse)?.userText
    let userInfo = response.notification.request.content.userInfo
    let route = NotificationActions.route(
      action: response.actionIdentifier, userInfo: userInfo, text: text)
    let session = userInfo[NotificationActions.sessionKey] as? String
    if let route {
      handle(route)
    } else if response.actionIdentifier == UNNotificationDefaultActionIdentifier, let session {
      show(session)
    }
  }

  /// A note that arrives while cox is frontmost still shows: the session it names may be in a
  /// window behind this one, and `notify` posts only what needs the person.
  public func userNotificationCenter(
    _ center: UNUserNotificationCenter, willPresent notification: UNNotification
  ) async -> UNNotificationPresentationOptions {
    [.banner, .list, .sound]
  }
}
