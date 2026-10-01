import SwiftUI

struct DurationView: View {
  @State private var open = false

  var body: some View {
    Text("Hello")
      .animation(.easeIn(duration: 0.2), value: open)
  }
}
