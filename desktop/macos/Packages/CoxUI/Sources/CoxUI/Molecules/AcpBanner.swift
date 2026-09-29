// `AcpBanner` (DT§3.3.1, mockup 27's `.notice`, T52.8): the line an external agent's transcript
// opens with — who drives the session, and that its own model, auth and billing apply. Separate
// from `NoticeRow` only to fix the wording in one place; the shape is the notice's.

import SwiftUI

/// A `NoticeRow` with the plug symbol, naming the agent over the Agent Client Protocol.
public struct AcpBanner: View {
  /// What the UI calls the agent: "Claude Agent", never "Claude Code".
  let agent: String

  public init(agent: String) { self.agent = agent }

  public var body: some View {
    NoticeRow(
      "This session is driven by \(agent) over the Agent Client Protocol. Its own model, auth "
        + "and billing apply; cox renders the stream and answers approvals.",
      symbol: "powerplug")
  }
}

#Preview("banner") { PreviewMatrix { AcpBannerSample() } }
