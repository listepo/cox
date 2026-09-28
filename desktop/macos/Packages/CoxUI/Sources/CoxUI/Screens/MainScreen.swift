// `MainScreen` (DS§4, DS§6.5; DT§5.1): the window shell — the sidebar, the toolbar, the
// transcript column and the inspector as floating panes on the window's glass. Composition
// only (DS§5): the panes style themselves, and the transcript and inspector tabs are slots
// their own cards fill (T37.23, T37.24, T37.29). The app binds `state` and `send` to its stores.

import SwiftUI

/// What the shell shows: each pane's state and which panes are out.
struct MainScreenState: Equatable, Sendable {
  var sidebar = Sidebar.State()
  var toolbar = SessionToolbar.State()
  var isSidebarVisible = true
  var isInspectorVisible = true
  var inspectorTab = InspectorTab.changes
}

/// Every intent the shell reports, tagged by the pane it came from.
enum MainScreenIntent: Equatable, Sendable {
  case sidebar(Sidebar.Intent)
  case toolbar(SessionToolbar.Intent)
  case inspectorTab(InspectorTab)
}

/// The shell for one open session. The sidebar and inspector fold away as `state` says; the
/// material, window opacity and Depth reach every pane from `coxAppearance`.
struct MainScreen<Transcript: View, InspectorContent: View>: View {
  let state: MainScreenState
  let send: (MainScreenIntent) -> Void
  let transcript: Transcript
  let inspector: (InspectorTab) -> InspectorContent

  init(
    state: MainScreenState, send: @escaping (MainScreenIntent) -> Void,
    @ViewBuilder transcript: () -> Transcript,
    @ViewBuilder inspector: @escaping (InspectorTab) -> InspectorContent
  ) {
    self.state = state
    self.send = send
    self.transcript = transcript()
    self.inspector = inspector
  }

  var body: some View {
    ShellPane(.window) {
      HStack(spacing: Size.paneGap) {
        if state.isSidebarVisible {
          Sidebar(state: state.sidebar) { send(.sidebar($0)) }
            .padding([.leading, .vertical], Size.paneGap)
        }
        VStack(spacing: 0) {
          SessionToolbar(
            state: state.toolbar, isSidebarVisible: state.isSidebarVisible,
            isInspectorVisible: state.isInspectorVisible
          ) { send(.toolbar($0)) }
          HStack(spacing: Size.paneGap) {
            ShellPane(.column) { transcript.frame(maxWidth: .infinity, maxHeight: .infinity) }
            if state.isInspectorVisible {
              Inspector(selection: state.inspectorTab, content: inspector(state.inspectorTab)) {
                send(.inspectorTab($0))
              }
            }
          }
          .padding([.trailing, .bottom], Size.paneGap)
          .padding(.leading, state.isSidebarVisible ? 0 : Size.paneGap)
        }
      }
    }
    .frame(minWidth: Size.windowMinWidth, minHeight: Size.windowMinHeight)
    .animation(.cox(Motion.durationSlow), value: state.isSidebarVisible)
    .animation(.cox(Motion.durationSlow), value: state.isInspectorVisible)
  }
}

#Preview("main") {
  MainScreenSample(state: PreviewState.main)
    .frame(width: PreviewState.window.width, height: PreviewState.window.height)
    .padding(Space.xxl)
    .background(PreviewBackdrop())
}
#Preview("panes folded") {
  MainScreenSample(state: PreviewState.folded)
    .frame(width: PreviewState.window.width, height: PreviewState.window.height)
    .padding(Space.xxl)
    .background(PreviewBackdrop())
}
