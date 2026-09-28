// `CapsuleStyle`'s check (T37.19.2): one snapshot per emphasis (plain, active) × light/dark ×
// Solid/Frosted, each showing rest, hovered, pressed and disabled.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct CapsuleStyleSnapshotTests {
  @Test(arguments: CapsuleStyle.Emphasis.allCases, Variant.all)
  func capsule(_ emphasis: CapsuleStyle.Emphasis, _ variant: Variant) throws {
    try StyleSnapshot.check(
      CapsuleSample(emphasis: emphasis), named: "\(emphasis)-\(variant.name)", variant)
  }
}

/// One emphasis in every state, on a pane as capsules sit in the toolbar.
private struct CapsuleSample: View {
  let emphasis: CapsuleStyle.Emphasis

  var body: some View {
    HStack(spacing: Space.l) {
      ForEach(ControlState.allCases, id: \.self) { state in
        CapsuleFace(label: Text("claude-opus-5"), emphasis: emphasis, state: state)
      }
    }
    .padding(Space.xl)
    .glassPane(RoundedRectangle(cornerRadius: Radius.pane, style: .continuous))
  }
}
