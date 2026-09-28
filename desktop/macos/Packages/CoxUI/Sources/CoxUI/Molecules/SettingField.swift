// `SettingField` (DS§6.3 row `SettingField`, the mockup's Settings `.sel` well and its Add key /
// Change key): a setting's text, or a provider's key, typed in a sunken well and sent on
// Return. Separate so a text value and a secret take one field, and so what is typed stays in
// the field until the person sends it instead of writing the config on every keystroke.

import SwiftUI

/// `value` in a `fill.primary` well, `Size.sidebarWidth` wide. Typing is local until Return
/// hands it to `commit`. A secure field never shows what is stored: it starts empty, and
/// empties again once sent.
struct SettingField: View {
  let value: String
  let prompt: String
  let isSecure: Bool
  let commit: (String) -> Void
  @State private var draft: String?

  init(
    _ value: String, prompt: String, isSecure: Bool = false,
    commit: @escaping (String) -> Void
  ) {
    self.value = value
    self.prompt = prompt
    self.isSecure = isSecure
    self.commit = commit
  }

  var body: some View {
    let text = Binding(get: { draft ?? (isSecure ? "" : value) }, set: { draft = $0 })
    // The prompt in `text.tertiary`, as `SessionFilter` draws it; the label names the field.
    let hint = Text(prompt).foregroundStyle(Color(.textTertiary))
    Group {
      if isSecure {
        SecureField(prompt, text: text, prompt: hint)
      } else {
        TextField(prompt, text: text, prompt: hint)
      }
    }
    .textFieldStyle(.plain)
    .textStyle(.body)
    .foregroundStyle(Color(.textPrimary))
    .lineLimit(1)
    .onSubmit {
      if let draft { commit(draft) }
      draft = nil
    }
    .padding(.horizontal, Space.ml)
    .frame(width: Size.sidebarWidth, height: Size.buttonHeightSmall)
    .insetWell(Color(.fillPrimary), cornerRadius: Radius.m)
  }
}

#Preview("text") {
  PreviewMatrix { SettingField(PreviewState.fieldText, prompt: PreviewState.fieldPrompt) { _ in } }
}
#Preview("secure") {
  PreviewMatrix { SettingField("", prompt: PreviewState.keyPrompt, isSecure: true) { _ in } }
}
