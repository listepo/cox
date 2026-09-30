// The browser pane's bar (T51.10): the address as mockup 25 shows it, scheme dropped, and the
// lock for `https` alone. A page that is not on the web shows its whole address, unlocked.

import Foundation
import Testing

@testable import CoxModel

@Test func theAddressDropsItsSchemeAndOnlyHttpsIsLocked() throws {
  let local = BrowserBarState(url: URL(string: "http://localhost:5173/checkout"), canGoBack: true)
  #expect(local.address == "localhost:5173/checkout")
  #expect(!local.isSecure)
  #expect(local.canGoBack)

  let secure = BrowserBarState(url: URL(string: "https://shop.example.com/"), isLoading: true)
  #expect(secure.address == "shop.example.com")
  #expect(secure.isSecure)
  #expect(secure.isLoading)

  let query = BrowserBarState(url: URL(string: "https://example.com/?q=1"))
  #expect(query.address == "example.com/?q=1")
}

@Test func beforeTheFirstLoadTheFieldIsEmpty() {
  #expect(BrowserBarState(url: nil).address == "")
  let blank = BrowserBarState(url: URL(string: "about:blank"))
  #expect(blank.address == "about:blank")
  #expect(!blank.isSecure)
}
