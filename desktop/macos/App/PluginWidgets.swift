// The plugin slots as CoxUI draws them (PL§8, T52.17): CoxClient's `PluginView` mapped onto
// CoxUI's `PluginWidget`, and which slots the session window shows. Separate because CoxUI
// imports no cox package, so the app is the one place the two meet. The trees were sanitized
// and bounded in `cox_app::plugin_ui` before they crossed the FFI.

import CoxClient
import CoxModel
import CoxUI

enum PluginWidgets {
  /// The toolbar's segments: every `status.left`, then every `status.right`.
  @MainActor static func status(_ store: SessionStore) -> [PluginWidget] {
    (store.pluginViews(.statusLeft) + store.pluginViews(.statusRight)).compactMap(widget)
  }

  /// The panels shown above the composer.
  @MainActor static func panels(_ store: SessionStore) -> [PluginPanelItem] {
    store.pluginViews(.panel).compactMap { slot in
      widget(slot).map { PluginPanelItem(plugin: slot.plugin, widget: $0) }
    }
  }

  /// The overlay shown, if any; the core shows at most one.
  @MainActor static func overlay(_ store: SessionStore) -> PluginWidget? {
    store.pluginViews(.overlay).first.flatMap(widget)
  }

  private static func widget(_ slot: PluginSlot) -> PluginWidget? {
    slot.view.map(PluginWidget.init)
  }
}

extension PluginWidget {
  init(_ view: PluginView) {
    let line = { (runs: [PluginRun]) in runs.map(PluginSpan.init) }
    switch view {
    case .text(let lines): self = .text(lines.map(line))
    case .list(let items, let selected):
      self = .list(items: items.map(line), selected: selected.map(Int.init))
    case .table(let header, let rows): self = .table(header: line(header), rows: rows.map(line))
    case .keyValue(let rows):
      self = .keyValue(rows.map { .init(key: PluginSpan($0.key), value: line($0.value)) })
    case .gauge(let ratio, let label): self = .gauge(ratio: ratio, label: PluginSpan(label))
    case .stack(let vertical, let children):
      self = .stack(vertical: vertical, children: children.map(PluginWidget.init))
    case .block(let title, let child):
      // The FFI carries the one child as a list; an empty block draws its title over nothing.
      self = .block(
        title: title.map(PluginSpan.init),
        child: child.first.map(PluginWidget.init) ?? .text([]))
    }
  }
}

extension PluginSpan {
  init(_ run: PluginRun) {
    self.init(run.text, Role(run.token), isBold: run.bold, isItalic: run.italic)
  }
}

extension PluginSpan.Role {
  init(_ token: StyleToken) {
    switch token {
    case .text: self = .text
    case .dim: self = .dim
    case .accent: self = .accent
    case .user: self = .user
    case .agent: self = .agent
    case .tool: self = .tool
    case .ok: self = .ok
    case .warn: self = .warn
    case .error: self = .error
    case .diffAdd: self = .diffAdd
    case .diffDel: self = .diffDel
    case .diffHunk: self = .diffHunk
    case .border: self = .border
    case .selection: self = .selection
    }
  }
}
