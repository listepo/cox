// `PreviewState` fixtures for `BrowserPaneChrome` (T51.10): mockup 25's local checkout page,
// idle, while it loads, and the same page over `https`, with the page drawn as text where the
// app shows WebKit's view. Separate so the organism's fixtures do not edit the shared file.

import SwiftUI

extension PreviewState {
  static let browserIdle = BrowserPaneState(address: "localhost:5173/checkout", canGoBack: true)

  static let browserLoading = BrowserPaneState(
    address: "localhost:5173/checkout", isLoading: true, canGoBack: true)

  static let browserSecure = BrowserPaneState(
    address: "shop.example.com/checkout", isSecure: true, canGoBack: true)

  /// Mockup 25's page, as lines.
  static let browserPage = ["acme", "Checkout", "Email", "Card number", "Expiry", "Pay $48.00"]
}

/// The pane at mockup 25's width, a page of text in WebKit's place.
struct BrowserPaneSample: View {
  let state: BrowserPaneState

  /// Mockup 25's web column: `width: 560px`.
  static let width: CGFloat = 560
  /// Tall enough for the bar and a page under it.
  static let height: CGFloat = 420

  var body: some View {
    BrowserPaneChrome(state: state) { _ in
    } content: {
      VStack(alignment: .leading, spacing: Space.m) {
        ForEach(PreviewState.browserPage, id: \.self) { Text($0) }
      }
      .textStyle(.body)
      .foregroundStyle(Color(.textPrimary))
      .padding(Space.xxl)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      .background(Color(.surfaceWindow))
    }
    .frame(width: Self.width, height: Self.height)
  }
}
