// `Spinner` and `ProgressRing`'s check (T37.20.1, DS§6.2): a snapshot per variant × light/dark
// × Solid/Frosted, the spinner turning unless Reduce Motion is on, and the ring clamping its
// fraction.

import Foundation
import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct ProgressAtomTests {
  /// Drawn still, so the image does not depend on how long the first frame took.
  @Test(arguments: Variant.all) func spinner(_ variant: Variant) throws {
    try assertCoxSnapshot(
      PreviewPane { Spinner().environment(\._accessibilityReduceMotion, true) }, variant,
      named: variant.name)
  }

  @Test(arguments: Variant.all) func progressRing(_ variant: Variant) throws {
    for fraction in PreviewState.fractions {
      try assertCoxSnapshot(
        PreviewPane { ProgressRing(fraction) }, variant,
        named: "\(Int(fraction * 100)).\(variant.name)")
    }
  }

  @Test func spinnerTurns() throws {
    #expect(try frames(reduceMotion: false).count > 1)
  }

  @Test func spinnerHoldsStillUnderReduceMotion() throws {
    #expect(try frames(reduceMotion: true).count == 1)
  }

  @Test func progressRingClampsItsFraction() {
    #expect(ProgressRing(1.5).fraction == 1)
    #expect(ProgressRing(-0.5).fraction == 0)
    #expect(ProgressRing(.nan).fraction == 0)
  }

  /// The distinct frames a spinner draws over more than half a turn.
  private func frames(reduceMotion: Bool) throws -> Set<Data> {
    let host = SnapshotHost(
      PreviewPane { Spinner() }, Variant(scheme: .light, material: .solid),
      reduceMotion: reduceMotion)
    var frames: Set<Data> = []
    let end = Date().addingTimeInterval(Motion.durationSlow * 2)
    while Date() < end {
      RunLoop.main.run(until: Date().addingTimeInterval(Motion.durationFast))
      if let frame = try host.bitmap().tiffRepresentation { frames.insert(frame) }
    }
    return frames
  }
}
