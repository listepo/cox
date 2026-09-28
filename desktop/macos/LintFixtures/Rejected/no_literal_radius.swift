import SwiftUI

struct RadiusView: View {
  var body: some View {
    Text("Hello")
      .background(Color.surfacePane, in: .rect(cornerRadius: 8))
  }
}
