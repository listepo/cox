import SwiftUI

struct ShadowView: View {
  var body: some View {
    Text("Hello")
      .shadow(color: Color.shadowTint, radius: Radius.s)
  }
}
