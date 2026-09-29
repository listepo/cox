// `PreviewState` fixtures for `CommandPalette` (T37.44.13): mockup 12's `rev` over an action, two
// sessions, a command and a file; the palette as ⌘K opens it; and a query nothing matches.
// Separate so the organism's fixtures do not edit the shared file.

extension PreviewState {
  static let paletteReview = CommandPalette.State(
    query: "rev",
    sections: [
      .init(
        title: "Actions",
        rows: [
          .init(
            id: "review", symbol: .system("eye"), title: "Review changes", matched: [0, 1, 2],
            detail: "⌘⇧R"),
          .init(
            id: "rewind", symbol: .system("arrow.uturn.backward"), title: "Rewind to turn…",
            detail: "code, conversation or both"),
          .init(
            id: "revert", symbol: .system("arrow.clockwise"), title: "Revert file to checkpoint…",
            matched: [0, 1, 2]),
        ]),
      .init(
        title: "Sessions",
        rows: [
          .init(
            id: "s-plugin", symbol: .system("bubble.left"), title: "Code review for plugin loader",
            matched: [5, 6, 7], detail: "cox · last week"),
          .init(
            id: "s-sitemap", symbol: .system("bubble.left"),
            title: "Revert broken sitemap change", matched: [0, 1, 2], detail: "acme-web · Sep 12"),
        ]),
      .init(
        title: "Commands & files",
        rows: [
          .init(
            id: "/code-review", symbol: .glyph("/"), title: "/code-review", matched: [6, 7, 8],
            detail: "skill"),
          .init(
            id: "@docs/design/review.md", symbol: .system("doc"), title: "docs/design/review.md",
            matched: [12, 13, 14], detail: "file"),
        ]),
    ],
    selection: "review")

  static let paletteOpen = CommandPalette.State(
    sections: [
      .init(
        title: "Actions",
        rows: [
          .init(id: "new", symbol: .system("plus"), title: "New session", detail: "⌘N"),
          .init(id: "review", symbol: .system("eye"), title: "Review changes", detail: "⌘⇧R"),
        ]),
      .init(
        title: "Sessions",
        rows: [
          .init(
            id: "s-jitter", symbol: .system("bubble.left"),
            title: "Add retry jitter to HTTP client", detail: "cox · 2 hr. ago")
        ]),
    ],
    selection: "new")

  static let paletteNoMatch = CommandPalette.State(query: "zzqx")
}
