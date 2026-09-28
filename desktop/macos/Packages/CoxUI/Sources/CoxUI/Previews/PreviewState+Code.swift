// `PreviewState` fixtures for the code molecules (T37.21.2, T37.21.3): the retry-jitter diff and
// code from mockup screen 28, as highlighted runs the core would send. Separate from
// `PreviewState.swift` so molecules built in parallel add their fixtures without editing one file.

import SwiftUI

extension PreviewState {
  static let hunkHeader = "@@ -41,12 +41,26 @@ impl Backoff"

  /// The mockup's `retryDiff`: context, one removed line, four added lines, context.
  static let hunk: [DiffLineView.Line] = [
    .init(
      kind: .context, number: "41",
      runs: [
        .init("pub fn", .keyword), .init(" "), .init("delay", .function), .init("(&"),
        .init("self", .keyword), .init(", attempt: "), .init("u32", .type), .init(") -> "),
        .init("Duration", .type), .init(" {"),
      ]),
    .init(
      kind: .removed, number: "42",
      runs: [
        .init("    "), .init("self", .keyword), .init(".base * "), .init("2", .number),
        .init("u32", .keyword), .init(".pow(attempt)"),
      ]),
    .init(
      kind: .added, number: "42",
      runs: [
        .init("    "), .init("let", .keyword), .init(" ceiling = "), .init("self", .keyword),
        .init(".base.saturating_mul("), .init("1", .number), .init(" << attempt.min("),
        .init("16", .number), .init("));"),
      ]),
    .init(
      kind: .added, number: "43",
      runs: [
        .init("    "), .init("let", .keyword), .init(" capped = ceiling.min("),
        .init("self", .keyword), .init(".cap);"),
      ]),
    .init(
      kind: .added, number: "44",
      runs: [
        .init("    "),
        .init("// full jitter: uniform in [0, capped], so retries spread out", .comment),
      ]),
    .init(
      kind: .added, number: "45",
      runs: [
        .init("    capped.mul_f64("), .init("self", .keyword), .init(".rng.lock().gen::<"),
        .init("f64", .type), .init(">())"),
      ]),
    .init(kind: .context, number: "46", runs: [.init("}")]),
  ]
}

/// The mockup's first line of each kind across the reading column.
struct DiffLineSample: View {
  let kind: DiffLineView.Kind

  var body: some View {
    let line = PreviewState.hunk.first { $0.kind == kind } ?? PreviewState.hunk[0]
    DiffLineView(line).frame(width: Size.readingWidth)
  }
}

/// The mockup's hunk across the reading column.
struct DiffHunkSample: View {
  var body: some View {
    DiffHunkView(header: PreviewState.hunkHeader, lines: PreviewState.hunk)
      .frame(width: Size.readingWidth)
  }
}
