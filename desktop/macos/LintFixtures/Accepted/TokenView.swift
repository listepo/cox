import SwiftUI

/// Every banned kind of literal, spelled through tokens and foundations instead.
struct TokenView: View {
  @State private var open = false

  var body: some View {
    VStack(spacing: 0) {
      // A comment may say .padding(12) or Color(red: 1, green: 0, blue: 0).
      Text("A string may say .padding(12) too")
        .textStyle(.body)
        .foregroundStyle(Color.textPrimary)
        .padding(.horizontal, Space.m)
        .frame(maxWidth: .infinity, minHeight: Size.buttonHeight)
        .background(Color.surfacePane, in: .rect(cornerRadius: Radius.m))
        .elevation(.e2)
        .padding(0)
    }
    .animation(Motion.standard, value: open)
  }
}
