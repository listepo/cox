// swift-tools-version: 6.2
// CoxModel (DT§4.6): the app's state without the Rust core. `CoxClient` is
// the contract — the timeline value types, the `CoreClient` protocol and the
// fixture client — and `CoxModel` the `@Observable` stores over it. The
// protocol lives here, not in CoxCore, because a package that declares the
// `CoxFFI` binary target fails to load until the XCFramework is built, and
// these tests and every SwiftUI preview must run without Rust (DT§8).
import PackageDescription

let package = Package(
    name: "CoxModel",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "CoxClient", targets: ["CoxClient"]),
        .library(name: "CoxModel", targets: ["CoxModel"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-collections", from: "1.7.1"),
    ],
    targets: [
        .target(name: "CoxClient"),
        .target(
            name: "CoxModel",
            dependencies: [
                "CoxClient",
                .product(name: "OrderedCollections", package: "swift-collections"),
            ]
        ),
        .testTarget(name: "CoxModelTests", dependencies: ["CoxModel"]),
    ]
)
