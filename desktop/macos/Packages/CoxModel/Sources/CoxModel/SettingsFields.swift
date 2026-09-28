// The Settings screen's fields (DT§5.7): a sidebar group split into one box per config table,
// each field with its title, its detail line and the control its kind and value give it, and
// an edit typed back by the field's kind. Here, not in CoxUI, because these decide what the
// screen shows (DS§1) and CoxUI depends on no cox package; the app copies them into CoxUI's
// `SettingsScreenState` case for case.

import CoxClient
import Foundation
import OrderedCollections

/// What a field shows; CoxUI's `SettingsScreen.Control` has the same cases.
public enum SettingControl: Equatable, Sendable {
  case toggle(Bool)
  /// A number the schema bounds on both ends, and the value as its JSON reads.
  case slider(Double, range: ClosedRange<Double>, text: String)
  /// A few options side by side.
  case choice(String, options: [String])
  /// A pop-up: more options than fit side by side, or a tier's model from the catalog.
  case menu(String, options: [SettingOption])
  /// Text, or a number typed as text; `SettingsStore.edit` types it back.
  case field(String)
  /// A list or an open shape, as its JSON: Settings does not edit it.
  case json(String)
}

/// A pop-up's option: the value sent, and what the menu calls it.
public struct SettingOption: Equatable, Sendable {
  public let value: String
  public let title: String

  public init(value: String, title: String) { (self.value, self.title) = (value, title) }
}

public struct SettingsField: Identifiable, Equatable, Sendable {
  public let setting: Setting
  /// The key's last segment in words: `base_url` → `Base url`.
  public let title: String
  /// For a value the project sets, the file it is set in; otherwise the schema's help text.
  public let detail: String?
  public let control: SettingControl
  public var id: String { setting.key }
}

/// One box on a group's page: the settings of one config table.
public struct SettingsTable: Identifiable, Equatable, Sendable {
  /// The keys' shared prefix, `tiers.code`.
  public let name: String
  public let fields: [SettingsField]
  /// The provider section whose key the box takes, for a `providers.<name>` table.
  public let provider: String?
  public var id: String { name }
}

extension SettingsStore {
  /// `section`'s settings by table, each table where its first key sorts.
  public func tables(in section: SettingsSection) -> [SettingsTable] {
    let byTable = OrderedDictionary(grouping: section.settings) {
      $0.key.split(separator: ".").dropLast().joined(separator: ".")
    }
    return byTable.map { name, settings in
      let parts = name.split(separator: ".")
      return SettingsTable(
        name: name,
        fields: settings.map {
          SettingsField(
            setting: $0, title: Self.title(of: $0.key), detail: detail(of: $0),
            control: control(of: $0))
        },
        provider: parts.count == 2 && parts[0] == "providers" ? String(parts[1]) : nil)
    }
  }

  /// Sends `input` typed as `key`'s kind wants it: a number for a number, a whole number for an
  /// integer. Text that is no number goes as text, so Rust's loader says why it is refused.
  public func edit(_ key: String, _ input: SettingValue) async {
    let kind = view?.settings.first { $0.key == key }?.kind
    var trimmed: String?
    if case .text(let text) = input { trimmed = text.trimmingCharacters(in: .whitespaces) }
    let value: SettingValue =
      switch (kind, input) {
      case (.integer, .number(let number)): .integer(Int64(number.rounded()))
      case (.integer, .text): trimmed.flatMap { Int64($0) }.map { .integer($0) } ?? input
      case (.number, .text): trimmed.flatMap { Double($0) }.map { .number($0) } ?? input
      default: input
      }
    await set(key, to: value)
  }

  private func detail(of setting: Setting) -> String? {
    if setting.layer == .project, let file = view?.projectFile { return "Set in \(file)" }
    return setting.description.isEmpty ? nil : setting.description
  }

  private static func title(of key: String) -> String {
    let words = (key.split(separator: ".").last ?? "").replacingOccurrences(of: "_", with: " ")
    return words.prefix(1).uppercased() + words.dropFirst()
  }

  /// More options than this take a pop-up rather than a segmented control, as mockup 18's
  /// pop-ups and segments show.
  static let segmentLimit = 3

  private func control(of setting: Setting) -> SettingControl {
    let json = Data(setting.value.utf8)
    func decoded<T: Decodable>(_: T.Type) -> T? { try? JSONDecoder().decode(T.self, from: json) }
    switch setting.kind {
    case .toggle:
      return decoded(Bool.self).map { .toggle($0) } ?? .json(setting.value)
    case .number(let min?, let max?) where min < max:
      let slider = decoded(Double.self).map {
        SettingControl.slider($0, range: min...max, text: setting.value)
      }
      return slider ?? .json(setting.value)
    case .integer, .number:
      return decoded(Double.self).map { _ in .field(setting.value) } ?? .field("")
    case .text:
      let text = decoded(String.self) ?? ""
      return modelMenu(setting.key, text) ?? .field(text)
    case .choice(let options) where options.count > Self.segmentLimit:
      return .menu(
        decoded(String.self) ?? "", options: options.map { .init(value: $0, title: $0) })
    case .choice(let options):
      return .choice(decoded(String.self) ?? "", options: options)
    case .list, .other:
      return .json(setting.value)
    }
  }

  /// `tiers.<tier>.model` as a pop-up of the catalog's models for that tier, by the name the
  /// toolbar shows; a value the catalog does not list stays first, so the pop-up shows it.
  private func modelMenu(_ key: String, _ value: String) -> SettingControl? {
    let parts = key.split(separator: ".")
    guard parts.count == 3, parts[0] == "tiers", parts[2] == "model",
      let tier = Tier(rawValue: String(parts[1]))
    else { return nil }
    var options = models.filter { $0.tier == tier }.map {
      SettingOption(value: $0.id, title: ModelName.short($0.displayName, id: $0.id))
    }
    guard !options.isEmpty else { return nil }
    if !value.isEmpty, !options.contains(where: { $0.value == value }) {
      options.insert(.init(value: value, title: value), at: 0)
    }
    return .menu(value, options: options)
  }
}
