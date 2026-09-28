// The rewind timeline's intent (T37.28.1, DT§5.4): a checkpoint of `ChangesTabState`, whose id
// is the turn, and the scope the person picked become `Intent.rewind`. Here, beside the mapping
// that wrote the id, so the app never parses it; the core does the rewind itself.

import CoxClient

extension SessionStore {
  /// Rewinds code, the conversation or both to before the checkpoint's turn. An id that is no
  /// turn — none that `ChangesTabState` writes — sends nothing.
  public func rewind(checkpoint id: String, code: Bool, conversation: Bool) async throws {
    guard let turn = UInt32(id) else { return }
    _ = try await send(.rewind(toTurn: turn, code: code, conversation: conversation))
  }

  /// The Changes tab's plain Rewind (`ChangesTab.Intent.rewind(checkpoint:)`): code only, DT§5.2's
  /// "Restore code to here" (A101). The timeline keeps all three scopes.
  public func rewind(checkpoint id: String) async throws {
    try await rewind(checkpoint: id, code: true, conversation: false)
  }
}
