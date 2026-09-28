// Which core the app talks to (DT§4.1, §8): the live Rust core, or a recorded patch stream from
// `desktop/macos/Fixtures` replayed by `FixtureCoreClient`, so the whole app runs without Rust
// state or a key. Separate from the window so the one launch-time choice has one owner.
//
//   Cox.app/Contents/MacOS/Cox -CoxFixture desktop/macos/Fixtures/edit.json
//   COX_HOME=/tmp/cox-scratch Cox.app/Contents/MacOS/Cox -CoxProject ~/src/repo

import CoxClient
import CoxCore
import CoxPlatform
import Foundation

enum LaunchCore {
  /// `-CoxFixture <path>` (a launch argument, read through `UserDefaults`' argument domain)
  /// replays that fixture; otherwise the live core at `COX_HOME`, `~/.cox` when unset.
  /// `COX_KEYRING=off`, as cargo sets it for every development run, keeps provider keys out of
  /// the Keychain here too (A49, A51): a dev launch never raises a Keychain prompt.
  static func pick(
    _ defaults: UserDefaults = .standard,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> Result<any CoreClient, any Error> {
    Result {
      if let path = defaults.string(forKey: "CoxFixture") {
        return FixtureCoreClient(fixture: try Fixture(contentsOf: URL(filePath: path)))
      }
      let secrets: any SecretStore =
        environment["COX_KEYRING"] == "off" ? MemorySecretStore() : KeychainSecretStore()
      return try LiveCoreClient(
        home: environment["COX_HOME"], host: HostBridge(MacHost(secrets: secrets)))
    }
  }

  /// The directory a new session works in: `-CoxProject <path>`, else the home directory (a
  /// Finder launch's working directory is `/`). The sidebar's project list replaces this.
  static func project(_ defaults: UserDefaults = .standard) -> String {
    defaults.string(forKey: "CoxProject") ?? NSHomeDirectory()
  }
}
