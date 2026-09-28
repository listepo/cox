// `Breadcrumb` (DS§6.3 row `Breadcrumb`, the mockup's `.crumb`): where the open session lives —
// its title, then its project and branch — at the leading end of the toolbar. Separate so the
// toolbar and any detached session window name a session the same way.

import SwiftUI

/// The title in `font.title.window`, a chevron, then the project and branch in `text.secondary`.
struct Breadcrumb: View {
  let title: String
  let project: String
  /// The worktree's branch, or `nil` outside git.
  let branch: String?

  init(_ title: String, project: String, branch: String? = nil) {
    self.title = title
    self.project = project
    self.branch = branch
  }

  var body: some View {
    HStack(spacing: Space.s) {
      Text(title)
        .textStyle(.titleWindow)
        .foregroundStyle(Color(.textPrimary))
        .lineLimit(1)
        .layoutPriority(1)
      Group {
        Image(systemName: "chevron.right").symbolStyle(.micro)
        Text(project).textStyle(.control)
        if let branch {
          Image(systemName: "arrow.triangle.branch").symbolStyle(.body)
          Text(branch).textStyle(.control).lineLimit(1).truncationMode(.middle)
        }
      }
      .foregroundStyle(Color(.textSecondary))
    }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel([title, project, branch].compactMap(\.self).joined(separator: ", "))
  }
}

#Preview("branch") { PreviewMatrix { BreadcrumbSample(branch: PreviewState.branch) } }
#Preview("no branch") { PreviewMatrix { BreadcrumbSample(branch: nil) } }
