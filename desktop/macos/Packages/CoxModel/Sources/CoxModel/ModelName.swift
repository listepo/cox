// A model's name as the pill shows it (T37.22.7, A111): the catalog's display name from models.dev
// without its vendor prefix, `Claude Sonnet 5` → `Sonnet 5`, else the id. Separate so the composer's
// chip and the toolbar pill cannot shorten one name two ways.

public enum ModelName {
  /// Brand words a vendor puts before every model it names, dropped because the pill already sits
  /// beside that vendor's session. Only where the rest still names the model on its own: `GPT-5.1`
  /// has no separate prefix, and `Grok 4.3` or `DeepSeek V4 Pro` without theirs would not.
  static let vendorPrefixes = ["Claude "]

  /// `name` without its vendor prefix; the id when there is no name.
  public static func short(_ name: String?, id: String) -> String {
    guard let name, !name.isEmpty else { return id }
    let prefix = vendorPrefixes.first { name.hasPrefix($0) && name.count > $0.count }
    return prefix.map { String(name.dropFirst($0.count)) } ?? name
  }
}
