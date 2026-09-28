// `UserBubble` (DS§6.3 row `UserBubble`, the mockup's `.user` and `.user .att`): what the user
// sent in a turn — the prompt and, under it, the pictures and files attached to it. Separate so
// every user turn in the transcript is one card, set apart from the assistant's prose.

import SwiftUI

/// The prompt in `font.transcript` on a readable bubble lifted to e2 (DS§3.4), then a row of
/// `Thumbnail`s.
struct UserBubble: View {
  /// One attachment as the core names it, with its picture when it is an image.
  struct Attachment: Identifiable, Equatable {
    let id: String
    var name: String
    /// The picture of an image attachment; `nil` for any other file.
    var image: Image?
  }

  let text: String
  let attachments: [Attachment]

  init(_ text: String, attachments: [Attachment] = []) {
    self.text = text
    self.attachments = attachments
  }

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
    VStack(alignment: .leading, spacing: Space.m) {
      Text(text)
        .textStyle(.transcript)
        .foregroundStyle(Color(.textPrimary))
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
      if !attachments.isEmpty {
        HStack(spacing: Space.m) {
          ForEach(attachments) { Thumbnail($0.name, image: $0.image) }
        }
      }
    }
    // The mockup's 14 px sides fall between `Space.l` and `Space.xl`; the smaller keeps the
    // bubble's text column as wide as the prose around it.
    .padding(.horizontal, Space.l)
    .padding(.vertical, Space.ml)
    // The mockup's `--fill` tint on a readable face, drawn behind the text so the glass
    // sweep lights the face and never washes out the prompt (DS§8).
    .background { shape.fill(Color(.fillPrimary)).glassPane(shape, role: .readable) }
    .elevation(.e2, cornerRadius: Radius.xl)
  }
}

#Preview("text") { PreviewMatrix { UserBubbleSample(hasAttachments: false) } }
#Preview("attachments") { PreviewMatrix { UserBubbleSample(hasAttachments: true) } }
