// A prompt's rewind menu (Figma frame 14-rewind-edit-resend): the turn it rewinds to before, how
// many files a code rewind restores, and the rewind or fork the menu picks. The rewind and the
// fork are the core's (`Intent.rewind`, `Intent.fork`); the restored-file count has no cox-app
// call yet, so `RewindPreviewService` is the seam and `MockRewindPreviewService` the frame's
// value until T37.47. The app copies the state into CoxUI's `RewindMenu.State`.

import CoxClient

/// How many files a code rewind to before a turn restores.
public protocol RewindPreviewService: Sendable {
  func restoredFiles(beforeTurn turn: UInt32) async throws -> Int
}

// MOCK: frame 14's "2 files restored" for every turn; T37.47 replaces this with the count the
// core reads from the checkpoints after the turn.
public struct MockRewindPreviewService: RewindPreviewService {
  public init() {}

  public func restoredFiles(beforeTurn turn: UInt32) async throws -> Int { 2 }
}

public struct RewindMenuState: Equatable, Sendable {
  public var turn: UInt32
  /// `nil` until the preview answers.
  public var restoredFiles: Int?

  public init(turn: UInt32, restoredFiles: Int? = nil) {
    (self.turn, self.restoredFiles) = (turn, restoredFiles)
  }
}

extension SessionStore {
  /// The menu for a prompt's turn, with the count `preview` gives; a failed preview leaves the
  /// count unknown rather than the menu closed.
  public func rewindMenu(turn: UInt32, preview: any RewindPreviewService) async -> RewindMenuState {
    RewindMenuState(turn: turn, restoredFiles: try? await preview.restoredFiles(beforeTurn: turn))
  }

  /// Fork a new session here: a child that starts from the conversation before `turn`.
  @discardableResult
  public func fork(beforeTurn turn: UInt32) async throws -> (any SessionClient)? {
    try await send(.fork(turn: turn))
  }
}
