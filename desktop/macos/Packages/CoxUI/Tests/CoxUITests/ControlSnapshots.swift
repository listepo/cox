// Rendering for the control snapshots (T37.19.3–T37.19.4): a control drawn over the colourful
// backdrop in one light/dark × material cell, through a window-hosted `NSHostingView` at 2×
// like FoundationsTests (whose helpers are private to that suite). Separate so the segmented,
// toggle and slider suites share one renderer.

import AppKit
import SnapshotTesting
import SwiftUI
import Testing

@testable import CoxUI

/// A control hosted in a borderless window, re-rendered as its root view changes.
@MainActor
struct ControlHost<Sample: View> {
  let host: NSHostingView<AnyView>
  let window: NSWindow

  init(_ sample: Sample, _ scheme: ColorScheme, _ material: GlassMaterial, reduceMotion: Bool) {
    host = NSHostingView(rootView: Self.dressed(sample, scheme, material, reduceMotion))
    host.appearance = NSAppearance(named: scheme == .dark ? .darkAqua : .aqua)
    host.frame = CGRect(origin: .zero, size: host.fittingSize)
    window = NSWindow(
      contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    host.layoutSubtreeIfNeeded()
  }

  /// Swaps in `sample`, keeping the view's identity so its animations run.
  func update(
    _ sample: Sample, _ scheme: ColorScheme, _ material: GlassMaterial, reduceMotion: Bool
  ) {
    host.rootView = Self.dressed(sample, scheme, material, reduceMotion)
    host.layoutSubtreeIfNeeded()
  }

  /// The current frame as a bitmap of a fixed 2× scale, independent of the display.
  func bitmap() throws -> NSBitmapImageRep {
    let bitmap = try #require(
      NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(host.bounds.width) * 2,
        pixelsHigh: Int(host.bounds.height) * 2, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0,
        bitsPerPixel: 0))
    bitmap.size = host.bounds.size
    host.cacheDisplay(in: host.bounds, to: bitmap)
    return bitmap
  }

  private static func dressed(
    _ sample: Sample, _ scheme: ColorScheme, _ material: GlassMaterial, _ reduceMotion: Bool
  ) -> AnyView {
    AnyView(
      sample
        .padding(Space.xxl)
        .background(ControlBackdrop())
        .environment(\.colorScheme, scheme)
        .environment(\.coxAppearance, Appearance(material: material))
        .environment(\._accessibilityReduceMotion, reduceMotion))
  }
}

/// Asserts `sample`'s snapshot in `variant`, named after the calling test and `state`.
@MainActor
func assertControlSnapshot(
  _ sample: some View, _ variant: Variant, state: String, testName: String = #function,
  filePath: StaticString = #filePath, fileID: StaticString = #fileID, line: UInt = #line
) throws {
  let bitmap = try ControlHost(sample, variant.scheme, variant.material, reduceMotion: false)
    .bitmap()
  let image = NSImage(size: bitmap.size)
  image.addRepresentation(bitmap)
  // Anti-aliasing differs slightly between machines; a real change moves far more pixels.
  assertSnapshot(
    of: image, as: .image(precision: 0.995, perceptualPrecision: 0.98),
    named: "\(state).\(variant.name)", fileID: fileID, file: filePath, testName: testName,
    line: line)
}

/// Colour behind every control, so glass and shadows show what they do to it.
private struct ControlBackdrop: View {
  var body: some View {
    LinearGradient(
      colors: [Color(.accent), Color(.statusPlan), Color(.statusSuccess)],
      startPoint: .topLeading, endPoint: .bottomTrailing)
  }
}
