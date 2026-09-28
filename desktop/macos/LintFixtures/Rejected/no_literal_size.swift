import SwiftUI

/// The card's Check: a literal padding fails lint.
struct PaddingView: View {
  var body: some View {
    Text("Hello")
      .padding(12)
  }
}
