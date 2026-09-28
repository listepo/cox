// The snapshot harness for the control styles (T37.19.1–T37.19.2): the same window-hosted 2×
// render `FoundationsTests` uses, callable from any test file, so each style's tests stay in
// their own file.

import AppKit
import SnapshotTesting
import SwiftUI
import Testing

@testable import CoxUI

@MainActor
enum StyleSnapshot {
  /// Asserts `sample` in `variant` against the image named `name` beside the calling test file.
  /// Anti-aliasing differs slightly between machines; a real change moves far more pixels.
  static func check(
    _ sample: some View, named name: String, _ variant: Variant,
    fileID: StaticString = #fileID, filePath: StaticString = #filePath,
    testName: String = #function, line: UInt = #line
  ) throws {
    let image = try render(sample, variant.scheme, Appearance(material: variant.material))
    assertSnapshot(
      of: image, as: .image(precision: 0.995, perceptualPrecision: 0.98), named: name,
      fileID: fileID, file: filePath, testName: testName, line: line)
  }

  /// Draws through a window-hosted `NSHostingView` (`ImageRenderer` drops glass content) into
  /// a bitmap of a fixed 2× scale, so the image does not depend on the machine's display.
  static func render(
    _ sample: some View, _ scheme: ColorScheme, _ appearance: Appearance
  ) throws -> NSImage {
    let host = NSHostingView(
      rootView:
        sample
        .padding(Space.xxl)
        .background(StyleBackdrop())
        .environment(\.colorScheme, scheme)
        .environment(\.coxAppearance, appearance))
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

/// Colour behind every sample, so glass and shadows show what they do to it.
private struct StyleBackdrop: View {
  var body: some View {
    LinearGradient(
      colors: [Color(.accent), Color(.statusPlan), Color(.statusSuccess)],
      startPoint: .topLeading, endPoint: .bottomTrailing)
  }
}
