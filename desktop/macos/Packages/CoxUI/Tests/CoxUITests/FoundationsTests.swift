// The Foundations' check (T37.19): one snapshot per modifier × light/dark × Solid/Frosted, the
// Reduce Transparency override, and the appearance arithmetic the modifiers share.

import AppKit
import SnapshotTesting
import SwiftUI
import Testing

@testable import CoxUI

/// One cell of the snapshot matrix.
struct Variant: CustomTestStringConvertible, Sendable {
  let scheme: ColorScheme
  let material: GlassMaterial

  static let all = [ColorScheme.light, .dark].flatMap { scheme in
    [GlassMaterial.solid, .frosted].map { Variant(scheme: scheme, material: $0) }
  }

  var name: String { "\(scheme == .dark ? "dark" : "light")-\(material.rawValue)" }
  var testDescription: String { name }
}

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
      let forced = try render(
        GlassPaneSample(), scheme, Appearance(material: .frosted), reduceTransparency: true)
      let solid = try render(GlassPaneSample(), scheme, Appearance(material: .solid))
      #expect(forced.tiffRepresentation == solid.tiffRepresentation)
    }
  }

  /// Anti-aliasing differs slightly between machines; a real change moves far more pixels.
  private func check(
    _ sample: some View, _ variant: Variant, testName: String = #function,
    filePath: StaticString = #filePath, line: UInt = #line
  ) throws {
    let image = try render(sample, variant.scheme, Appearance(material: variant.material))
    assertSnapshot(
      of: image, as: .image(precision: 0.995, perceptualPrecision: 0.98), named: variant.name,
      fileID: #fileID, file: filePath, testName: testName, line: line)
  }

  /// Draws through a window-hosted `NSHostingView` (`ImageRenderer` drops glass content) into
  /// a bitmap of a fixed 2× scale, so the image does not depend on the machine's display.
  private func render(
    _ sample: some View, _ scheme: ColorScheme, _ appearance: Appearance,
    reduceTransparency: Bool = false
  ) throws -> NSImage {
    let host = NSHostingView(
      rootView:
        sample
        .padding(Space.xxl)
        .background(Backdrop())
        .environment(\.colorScheme, scheme)
        .environment(\.coxAppearance, appearance)
        .environment(\._accessibilityReduceTransparency, reduceTransparency))
    host.appearance = NSAppearance(named: scheme == .dark ? .darkAqua : .aqua)
    host.frame = CGRect(origin: .zero, size: host.fittingSize)
    let window = NSWindow(
      contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    let bitmap = try #require(
      NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(host.bounds.width) * 2,
        pixelsHigh: Int(host.bounds.height) * 2, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0,
        bitsPerPixel: 0))
    bitmap.size = host.bounds.size
    host.cacheDisplay(in: host.bounds, to: bitmap)
    let image = NSImage(size: host.bounds.size)
    image.addRepresentation(bitmap)
    return image
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

/// Colour behind every sample, so glass and shadows show what they do to it.
private struct Backdrop: View {
  var body: some View {
    LinearGradient(
      colors: [Color(.accent), Color(.statusPlan), Color(.statusSuccess)],
      startPoint: .topLeading, endPoint: .bottomTrailing)
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
