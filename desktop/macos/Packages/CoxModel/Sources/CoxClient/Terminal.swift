// A terminal pane's shell as the views see it (T51.5, DT§3.2): cox-ffi's `TerminalHandle`
// reduced to bytes in and bytes out, so CoxPlatform's SwiftTerm view and its tests never load
// the XCFramework. The shell itself runs in Rust (`cox_app::terminal`): Swift never spawns a
// process, and what the shell prints never reaches the session's timeline.

/// One running terminal of a session, implemented over `TerminalHandle` by CoxCore.
public protocol TerminalClient: AnyObject, Sendable {
  /// Keys and pastes, as the terminal view encodes them.
  func write(_ bytes: [UInt8]) throws
  /// The pane's size in cells; the shell gets SIGWINCH.
  func resize(cols: UInt16, rows: UInt16) throws
  /// What the shell prints, batch by batch, until it exits. One consumer at a time.
  var outputs: AsyncStream<[UInt8]> { get }
  /// The shell's exit code once it exited.
  func exitStatus() -> UInt32?
  /// Hangs the shell up and kills its process groups.
  func close()
}
