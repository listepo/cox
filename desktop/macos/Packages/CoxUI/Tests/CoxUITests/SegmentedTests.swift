// `CoxSegmented`'s check (T37.19.3): a snapshot per selection × light/dark × Solid/Frosted, and
// the selection sliding between segments, or cross-fading under Reduce Motion.

import AppKit
import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct SegmentedTests {
  @Test(arguments: Variant.all) func selection(_ variant: Variant) throws {
    for mode in SegmentedSample.modes {
      try assertControlSnapshot(SegmentedSample(selection: mode), variant, state: mode)
    }
  }

  @Test func selectionSlidesThroughTheMiddleSegment() throws {
    let frames = try animate(reduceMotion: false)
    #expect(frames.contains { $0[1] > 0.5 })
    #expect(!frames.contains(where: Self.fadingAtBothEnds))
  }

  @Test func reduceMotionCrossFadesTheSelection() throws {
    let frames = try animate(reduceMotion: true)
    #expect(!frames.contains { $0[1] > 0.1 })
    #expect(frames.contains(where: Self.fadingAtBothEnds))
  }

  /// Mid-fade both ends hold part of the pill.
  private static func fadingAtBothEnds(_ cover: [Double]) -> Bool {
    [cover[0], cover[2]].allSatisfy { $0 > 0.1 && $0 < 0.9 }
  }

  /// Moves the selection from the first mode to the last and returns, for each frame in
  /// between, how much of the pill covers each segment (0 bare … 1 selected).
  private func animate(reduceMotion: Bool) throws -> [[Double]] {
    let (first, last) = (SegmentedSample.modes[0], SegmentedSample.modes[2])
    let control = ControlHost(
      SegmentedSample(selection: first), .light, .solid, reduceMotion: reduceMotion)
    let before = try control.bitmap()
    control.update(SegmentedSample(selection: last), .light, .solid, reduceMotion: reduceMotion)
    var frames: [NSBitmapImageRep] = []
    let end = Date().addingTimeInterval(Motion.durationBase * 2)
    while Date() < end {
      RunLoop.main.run(until: Date().addingTimeInterval(Motion.durationBase / 20))
      frames.append(try control.bitmap())
    }
    let probe = try PillProbe(before: before, after: try #require(frames.last))
    return frames.map(probe.cover)
  }
}

/// Reads the pill's cover in the middle of each segment, just above its title.
private struct PillProbe {
  let points: [(x: Int, y: Int)]
  let pill: NSColor
  let bare: [NSColor]

  /// `before` has the pill on the first segment, `after` on the last.
  init(before: NSBitmapImageRep, after: NSBitmapImageRep) throws {
    // Pixels at 2×; the control sits inside the backdrop's `Space.xxl` margin.
    let margin = Int(Space.xxl * 2)
    let control = before.pixelsWide - margin * 2
    let row = margin + Int((Space.xxs + Space.xs) * 2)
    let points = [1, 3, 5].map { (x: margin + control * $0 / 6, y: row) }
    self.points = points
    pill = try #require(before.colorAt(x: points[0].x, y: row))
    bare = try [after, before, before].enumerated().map { index, bitmap in
      try #require(bitmap.colorAt(x: points[index].x, y: row))
    }
  }

  func cover(_ frame: NSBitmapImageRep) -> [Double] {
    points.indices.map { index in
      let point = points[index]
      guard let color = frame.colorAt(x: point.x, y: point.y) else { return 0 }
      return Self.distance(color, bare[index]) / Self.distance(pill, bare[index])
    }
  }

  private static func distance(_ lhs: NSColor, _ rhs: NSColor) -> Double {
    abs(lhs.redComponent - rhs.redComponent) + abs(lhs.greenComponent - rhs.greenComponent)
      + abs(lhs.blueComponent - rhs.blueComponent)
  }
}

private struct SegmentedSample: View {
  static let modes = ["Ask", "Plan", "Auto"]
  let selection: String

  var body: some View {
    CoxSegmented("Mode", selection: .constant(selection), options: Self.modes) {
      Text(verbatim: $0)
    }
  }
}
