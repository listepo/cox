// `CoxToggleStyle` and `CoxSlider`'s check (T37.19.4): a snapshot per on/off and per slider
// value × light/dark × Solid/Frosted, and the slider placing its knob by the value's share of
// the range, clamped to it.

import AppKit
import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct ToggleSliderTests {
  @Test(arguments: Variant.all) func toggle(_ variant: Variant) throws {
    for isOn in [false, true] {
      try assertControlSnapshot(ToggleSample(isOn: isOn), variant, state: isOn ? "on" : "off")
    }
  }

  @Test(arguments: Variant.all) func slider(_ variant: Variant) throws {
    for value in [0, 0.5, 1] {
      try assertControlSnapshot(
        SliderSample(value: value), variant, state: "value\(Int(value * 100))")
    }
  }

  @Test func sliderPlacesTheKnobByTheValuesShareOfTheRange() throws {
    try #expect(
      render(SliderSample(value: 15, range: 10...20)) == render(SliderSample(value: 0.5)))
  }

  @Test func sliderClampsAValueOutsideTheRange() throws {
    try #expect(render(SliderSample(value: 1.5)) == render(SliderSample(value: 1)))
    try #expect(render(SliderSample(value: -1)) == render(SliderSample(value: 0)))
  }

  private func render(_ sample: SliderSample) throws -> Data? {
    try ControlHost(sample, .light, .solid, reduceMotion: false).bitmap().tiffRepresentation
  }
}

private struct ToggleSample: View {
  let isOn: Bool

  var body: some View {
    Toggle("Tint from wallpaper", isOn: .constant(isOn)).toggleStyle(CoxToggleStyle())
  }
}

private struct SliderSample: View {
  let value: Double
  var range: ClosedRange<Double> = 0...1

  var body: some View {
    CoxSlider("Depth", value: .constant(value), in: range).frame(width: Size.popoverWidth / 2)
  }
}
