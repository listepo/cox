// The Foundations' check (T37.19): one snapshot per modifier × light/dark × Solid/Frosted, the
// Reduce Transparency override, and the appearance arithmetic the modifiers share.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct FoundationsSnapshotTests {
  @Test(arguments: Variant.all) func elevation(_ variant: Variant) throws {
    try check(ElevationSample(), variant)
  }

  @Test(arguments: Variant.all) func glassPane(_ variant: Variant) throws {
    try check(GlassPaneSample(), variant)
  }

  @Test(arguments: Variant.all) func specular(_ variant: Variant) throws {
    try check(SpecularSample(), variant)
  }

  @Test(arguments: Variant.all) func hairline(_ variant: Variant) throws {
    try check(HairlineSample(), variant)
  }

  @Test(arguments: Variant.all) func insetWell(_ variant: Variant) throws {
    try check(InsetWellSample(), variant)
  }

  @Test(arguments: Variant.all) func textStyle(_ variant: Variant) throws {
    try check(TextStyleSample(), variant)
  }

  @Test func reduceTransparencyRendersSolid() throws {
    for scheme in [ColorScheme.light, .dark] {
      let forced = try SnapshotHost(
        GlassPaneSample(), Variant(scheme: scheme, material: .frosted), reduceTransparency: true
      ).bitmap()
      let solid = try SnapshotHost(GlassPaneSample(), Variant(scheme: scheme, material: .solid))
        .bitmap()
      #expect(forced.tiffRepresentation == solid.tiffRepresentation)
    }
  }

  private func check(_ sample: some View, _ variant: Variant, test: String = #function) throws {
    try assertCoxSnapshot(sample, variant, named: variant.name, testName: test)
  }
}

@Suite struct AppearanceTests {
  @Test func reduceTransparencyForcesSolidAndOpaque() {
    let forced = Appearance(material: .glossy).effective(reduceTransparency: true)
    #expect(forced.material == .solid)
    #expect(forced.backgroundOpacity(.chrome) == MaterialToken.solidWindowOpacity)
    #expect(forced.specular == MaterialToken.solidSpecular)
  }

  @Test func readableSurfacesNeverDropBelowTheFloor() {
    let clear = Appearance(material: .frosted, windowOpacity: 0)
    #expect(clear.backgroundOpacity(.chrome) == 0)
    #expect(clear.backgroundOpacity(.readable) == MaterialToken.readableFloorWindowOpacity)
  }

  @Test func windowOpacityDefaultsToTheMaterialToken() {
    #expect(Appearance(material: .glossy).windowOpacity == MaterialToken.glossyWindowOpacity)
    #expect(Appearance(material: .frosted).windowOpacity == MaterialToken.frostedWindowOpacity)
  }
}

private struct ElevationSample: View {
  let levels: [ElevationToken] = [.e0, .e1, .e2, .e3, .e4, .e5]

  var body: some View {
    HStack(spacing: Space.xxl) {
      ForEach(levels.indices, id: \.self) { index in
        RoundedRectangle(cornerRadius: Radius.l, style: .continuous)
          .fill(Color(.surfaceCapsule))
          .frame(width: Size.toolbarHeight, height: Size.toolbarHeight)
          .elevation(levels[index], cornerRadius: Radius.l)
      }
    }
    .padding(Space.huge)
    .background(Color(.surfaceWindow))
  }
}

private struct GlassPaneSample: View {
  var body: some View {
    HStack(spacing: Size.paneGap) {
      Text("Chrome pane")
        .frame(width: Size.popoverWidth / 2, height: Size.toolbarHeight * 2)
        .glassPane(RoundedRectangle(cornerRadius: Radius.pane, style: .continuous))
      Text("Readable pane")
        .frame(width: Size.popoverWidth / 2, height: Size.toolbarHeight * 2)
        .glassPane(
          RoundedRectangle(cornerRadius: Radius.pane, style: .continuous),
          surface: Color(.surfacePopover), role: .readable)
    }
    .textStyle(.body)
    .foregroundStyle(Color(.textPrimary))
  }
}

private struct SpecularSample: View {
  var body: some View {
    RoundedRectangle(cornerRadius: Radius.pane, style: .continuous)
      .fill(Color(.fillSecondary))
      .frame(width: Size.popoverWidth, height: Size.toolbarHeight * 2)
      .specular(MaterialToken.glossySpecular, in: .rect(cornerRadius: Radius.pane))
  }
}

private struct HairlineSample: View {
  var body: some View {
    HStack(spacing: Space.xxl) {
      Color(.surfaceWindow)
        .frame(width: Size.toolbarHeight * 2, height: Size.toolbarHeight)
        .hairline([.top, .bottom])
      Color(.surfaceWindow)
        .frame(width: Size.toolbarHeight * 2, height: Size.toolbarHeight)
        .clipShape(.capsule)
        .hairline(in: .capsule)
    }
  }
}

private struct InsetWellSample: View {
  var body: some View {
    VStack(alignment: .leading, spacing: Space.xs) {
      Text("$ cargo test")
      Text("test result: ok").foregroundStyle(Color(.textTerminalOk))
    }
    .textStyle(.monoTerminal)
    .foregroundStyle(Color(.textTerminal))
    .padding(Space.ml)
    .frame(width: Size.popoverWidth, alignment: .leading)
    .insetWell()
  }
}

private struct TextStyleSample: View {
  let tokens: [FontToken] = [.titleWindow, .transcript, .control, .label, .metric, .monoCode]

  var body: some View {
    VStack(alignment: .leading, spacing: Space.xs) {
      ForEach(tokens.indices, id: \.self) { index in
        Text("Tokens 0123456789").textStyle(tokens[index])
      }
    }
    .foregroundStyle(Color(.textPrimary))
    .padding(Space.l)
    .background(Color(.surfaceWindow))
  }
}
