// `Inspector` (DS§6.4 row `Inspector`, the mockup's `.insp`; DT§5.1): the pane at the trailing
// edge of the window — its title, the tab strip and the selected tab's content. Separate so the
// window shell owns the pane and its tabs while each tab's content is its own view (T37.29).

import SwiftUI

/// A floating `ShellPane(.inspector)` of `Size.inspectorWidth`: the title, the tabs with the
/// selected one lifted, then `content` for the selected tab (`ChangesTab`, …), scrolling.
struct Inspector<Content: View>: View {
  let selection: InspectorTab
  let select: (InspectorTab) -> Void
  let content: Content

  init(selection: InspectorTab, content: Content, select: @escaping (InspectorTab) -> Void) {
    self.selection = selection
    self.content = content
    self.select = select
  }

  var body: some View {
    ShellPane(.inspector) {
      VStack(alignment: .leading, spacing: 0) {
        Text("Inspector")
          .textStyle(.titleWindow)
          .foregroundStyle(Color(.textPrimary))
          .accessibilityAddTraits(.isHeader)
          .padding(.horizontal, Space.xl)
          .frame(height: Size.toolbarHeight)
        HStack(spacing: Space.xxs) {
          ForEach(InspectorTab.allCases, id: \.self) { tab in
            TabButton(title: tab.title, isSelected: tab == selection) { select(tab) }
          }
        }
        .padding(.horizontal, Space.l)
        .padding(.bottom, Space.ml)
        .frame(maxWidth: .infinity, alignment: .leading)
        .hairline(.bottom)
        // The mockup's `.ib`: every tab scrolls in the same inset body. Its first `.ih` sits
        // 18 pt under the tabs (14 of padding and 4 of its own margin): `space.xl` is nearest.
        ScrollView {
          content
            .padding(.horizontal, Space.xl)
            .padding(.top, Space.xl)
            .padding(.bottom, Space.l)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxHeight: .infinity)
      }
    }
    .frame(width: Size.inspectorWidth)
    .coxTransition(.move(edge: .trailing).combined(with: .opacity))
  }
}

/// The DT§5.1 tabs, in the order the strip shows them.
public enum InspectorTab: CaseIterable, Sendable {
  case changes, plan, context, tasks, info

  var title: String {
    switch self {
    case .changes: "Changes"
    case .plan: "Plan"
    case .context: "Context"
    case .tasks: "Tasks"
    case .info: "Info"
    }
  }
}

/// One tab: a quiet label, or lifted on the window surface while selected.
private struct TabButton: View {
  let title: String
  let isSelected: Bool
  let action: () -> Void

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.s, style: .continuous)
    Button(action: action) {
      Text(title)
        .textStyle(.control)
        .foregroundStyle(Color(isSelected ? .textPrimary : .textSecondary))
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.xs)
        .background { if isSelected { shape.fill(Color(.surfaceWindow)) } }
        .elevation(isSelected ? .e1 : .e0, cornerRadius: Radius.s)
        .contentShape(shape)
    }
    .buttonStyle(.plain)
    .accessibilityAddTraits(isSelected ? .isSelected : [])
  }
}

#Preview("empty tabs") {
  PreviewMatrix {
    Inspector(selection: .changes, content: EmptyView()) { _ in }
      .frame(height: Size.toolbarHeight * 2)
  }
}
