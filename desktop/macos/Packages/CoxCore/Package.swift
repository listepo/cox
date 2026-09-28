// swift-tools-version: 6.2
// CoxCore (DT§4.6): the Rust core behind the `CoreClient` protocol.
// `CoxFFI` is the XCFramework `just desktop-xcframework` builds;
// `CoxFFIBindings` is UniFFI's generated Swift over it, a symlink into the
// same gitignored build output, compiled in Swift 5 mode because its
// `Sendable` coverage is partial (research.md 9.3.4) — `CoxCore` wraps it, so
// that stays inside this package. Needs the XCFramework built first.
import PackageDescription

let package = Package(
    name: "CoxCore",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "CoxCore", targets: ["CoxCore"]),
    ],
    dependencies: [
        .package(path: "../CoxModel"),
    ],
    targets: [
        .binaryTarget(name: "CoxFFI", path: "../../build/CoxFFI.xcframework"),
        .target(
            name: "CoxFFIBindings",
            dependencies: ["CoxFFI"],
            swiftSettings: [.swiftLanguageMode(.v5)],
            // The static library's own system links: reqwest's proxy lookup
            // needs SystemConfiguration; Security comes through Foundation.
            linkerSettings: [.linkedFramework("SystemConfiguration")]
        ),
        .target(
            name: "CoxCore",
            dependencies: [
                "CoxFFIBindings",
                .product(name: "CoxClient", package: "CoxModel"),
            ]
        ),
        .testTarget(
            name: "CoxCoreTests",
            dependencies: [
                "CoxCore", "CoxFFIBindings",
                .product(name: "CoxClient", package: "CoxModel"),
            ]
        ),
    ]
)
