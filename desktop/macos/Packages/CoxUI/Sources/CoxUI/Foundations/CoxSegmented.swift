// `CoxSegmented` (DS§6.1, the mockup's `.seg`): the segmented control, a glass capsule whose
// selection is a pill lifted to e1 that slides between segments, or cross-fades under Reduce
// Motion. A view rather than a `PickerStyle`: SwiftUI has no public hook to restyle the
// segments of a picker on macOS, so this takes a picker's inputs and VoiceOver sees a picker.

import SwiftUI

/// Mutually exclusive `options`, each named by `title`, with `selection` lifted.
struct CoxSegmented<Option: Hashable>: View {
  let label: LocalizedStringKey
  @Binding var selection: Option
  let options: [Option]
  let title: (Option) -> Text
  @Namespace private var pill

  init(
    _ label: LocalizedStringKey, selection: Binding<Option>, options: [Option],
    title: @escaping (Option) -> Text
  ) {
    self.label = label
    self._selection = selection
    self.options = options
    self.title = title
  }

  var body: some View {
    HStack(spacing: Space.xxs) {
      ForEach(options, id: \.self) { option in
        Segment(title: title(option), isSelected: option == selection, pill: pill) {
          selection = option
        }
      }
    }
    .padding(Space.xxs)
    .frame(height: Size.capsuleHeight)
    .glassPane(Capsule(), surface: Color(.surfaceCapsule), role: .readable)
    .overlay { Capsule().strokeBorder(Color(.surfaceCapsuleBorder), lineWidth: Size.hairline) }
    .elevation(.e1, cornerRadius: Radius.capsule)
    .animation(.cox(Motion.durationBase), value: selection)
    .accessibilityRepresentation {
      Picker(label, selection: $selection) {
        ForEach(options, id: \.self) { title($0).tag($0) }
      }
      .pickerStyle(.segmented)
    }
  }
}

private struct Segment: View {
  let title: Text
  let isSelected: Bool
  let pill: Namespace.ID
  let select: () -> Void

  var body: some View {
    Button(action: select) {
      title
        .textStyle(.control)
        .foregroundStyle(Color(isSelected ? .textPrimary : .textSecondary))
        .padding(.horizontal, Space.ml)
        .frame(maxHeight: .infinity)
        .background {
          if isSelected { SelectionPill(pill: pill) }
        }
        .contentShape(Capsule())
    }
    .buttonStyle(.plain)
  }
}

/// The selected segment's lifted surface; one per control, so it moves rather than blinks.
private struct SelectionPill: View {
  let pill: Namespace.ID

  var body: some View {
    Capsule()
      .fill(Color(.surfaceWindow))
      .elevation(.e1, cornerRadius: Radius.capsule)
      .coxMatchedGeometry(id: 0, in: pill)
  }
}
