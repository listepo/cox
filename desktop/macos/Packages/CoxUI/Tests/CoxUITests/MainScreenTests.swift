// The window shell's check (T37.22, DS§4, DS§6.4): the main screen in Solid, Frosted and
// Glossy, light and dark, laid out as mockup screens 28–29 — sidebar, toolbar, transcript
// column and inspector as floating panes — and with both side panes folded; its organisms per
// light/dark × Solid/Frosted cell; and the pane table the shell is built from.

import AppKit
import SnapshotTesting
import SwiftUI
import Testing

@testable import CoxUI

extension Variant {
  /// The DS§9 matrix plus Glossy, the card's third material.
  static let withGlossy =
    all + [ColorScheme.light, .dark].map { Variant(scheme: $0, material: .glossy) }
}

@MainActor
@Suite struct MainScreenSnapshotTests {
  @Test(arguments: Variant.withGlossy) func mainScreen(_ variant: Variant) throws {
    try checkWindow(MainScreenSample(state: PreviewState.main), variant)
  }

  @Test func mainScreenWithPanesFolded() throws {
    try checkWindow(MainScreenSample(state: PreviewState.folded), Variant.all[1])
  }

  @Test(arguments: Variant.all) func sidebar(_ variant: Variant) throws {
    try assertCoxSnapshot(
      Sidebar(state: PreviewState.sidebar) { _ in }.frame(height: Size.windowMinHeight), variant,
      named: variant.name)
  }

  /// The folded main screen shows the bar with the sidebar toggle it gains.
  @Test(arguments: Variant.all) func sessionToolbar(_ variant: Variant) throws {
    try assertCoxSnapshot(
      PreviewPane { SessionToolbar(state: PreviewState.toolbar) { _ in }.fixedSize() }, variant,
      named: variant.name)
  }

  @Test(arguments: Variant.all) func inspectorFrame(_ variant: Variant) throws {
    try assertCoxSnapshot(
      Inspector(selection: .changes, content: EmptyView()) { _ in }
        .frame(height: Size.toolbarHeight * 2), variant, named: variant.name)
  }

  /// The whole window at 1× — its structure is the check, and the organisms' own snapshots
  /// hold the detail at 2× — so the committed images stay a few hundred kilobytes each.
  private func checkWindow(
    _ screen: some View, _ variant: Variant, test: String = #function
  ) throws {
    let sample = screen.frame(width: PreviewState.window.width, height: PreviewState.window.height)
    let full = try SnapshotHost(sample, variant).bitmap()
    let rep = try #require(
      NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(full.size.width), pixelsHigh: Int(full.size.height),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
    rep.size = full.size
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    full.draw(in: CGRect(origin: .zero, size: full.size))
    NSGraphicsContext.restoreGraphicsState()
    let image = NSImage(size: rep.size)
    image.addRepresentation(rep)
    assertSnapshot(
      of: image, as: .image(precision: 0.995, perceptualPrecision: 0.98), named: variant.name,
      testName: test)
  }
}

@Suite struct MainScreenTests {
  @Test func paneKindsFollowTheDepthAndRadiusScales() {
    let kinds = ShellPaneKind.allCases
    #expect(kinds.map(\.elevation) == [.e5, .e2, .e0, .e2])
    #expect(kinds.map(\.radius) == [Radius.window, Radius.panel, Radius.pane, Radius.pane])
  }

  @Test func inspectorTabsFollowTheDesignOrder() {
    #expect(InspectorTab.allCases.map(\.title) == ["Changes", "Plan", "Context", "Tasks", "Info"])
  }
}
