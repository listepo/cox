// The sidebar's "Needs you" rows (T37.27.7, DS§6.4 `Sidebar`): one row per inbox item, two of
// one session among them, per light/dark × Solid/Frosted cell; an item's row opens and
// highlights its session.

import Testing

@testable import CoxUI

@MainActor
@Suite struct SidebarInboxSnapshotTests {
  @Test(arguments: Variant.all) func needsYouSidebar(_ variant: Variant) throws {
    try assertCoxSnapshot(
      Sidebar(state: PreviewState.inboxSidebar) { _ in }.frame(height: Size.windowMinHeight),
      variant, named: variant.name)
  }
}

@Suite struct SidebarInboxTests {
  @Test func anItemRowOpensItsSessionAndAPlainRowItself() {
    let rows = PreviewState.inboxSidebar.groups[0].sessions
    #expect(rows.map(\.opens) == ["pkce", "pkce", "seo", "flaky"])
    #expect(rows.map(\.isReadOnly) == [false, false, false, true])
    #expect(PreviewState.sidebar.groups[0].sessions.map(\.opens) == ["pkce", "flaky"])
  }
}
