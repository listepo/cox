// The `CoreClient` over cox-ffi (DT§4.4, §4.6): opens sessions through the
// generated `App` and hands the stores `CoxClient` values. Separate from the
// conversions, which are data only; this file is the one place the app
// calls into Rust.

import CoxClient
import CoxFFIBindings

public final class LiveCoreClient: CoreClient {
    private let app: App

    /// `home` is `COX_HOME`; `nil` means `~/.cox`. Call `loadLoginEnv()`
    /// first, once per launch (DT§4.8).
    public init(home: String?, host: any AppHost) throws {
        app = try App(home: home, host: host)
    }

    public func open(_ request: OpenSession) async throws -> any SessionClient {
        let handle = try await app.open(
            request: OpenRequest(cwd: request.cwd, resume: request.resume, theme: request.theme))
        return LiveSession(handle)
    }
}

final class LiveSession: SessionClient {
    private let handle: SessionHandle

    init(_ handle: SessionHandle) { self.handle = handle }

    var id: String { handle.id() }

    func snapshot() -> [CoxClient.Block] { handle.snapshot().map { CoxClient.Block($0) } }

    func nextPatches() async -> [CoxClient.TimelinePatch]? {
        await handle.nextPatches()?.map { CoxClient.TimelinePatch($0) }
    }

    func send(_ intent: CoxClient.Intent) async throws -> (any SessionClient)? {
        try await handle.send(intent: CoxFFIBindings.Intent(intent)).map { LiveSession($0) }
    }

    func close() { handle.close() }
}
