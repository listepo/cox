// swift-tools-version: 6.2
// CoxPlatform (DT§4.6): what the app asks macOS for — today the provider
// keys in the Keychain over the Security framework, no wrapper package
// (research.md 9.5.8) — and notifications with Allow, Deny and Answer
// actions (T37.27); later Sparkle and the OAuth handoff.
// Depends on CoxClient only, for the `SecretStore` seam, so it builds and
// tests without the Rust XCFramework.
import PackageDescription

/// desktop/macos/.swiftlint.yml, through Packages/CoxPlatform/.swiftlint.yml.
let swiftLint = Target.PluginUsage.plugin(
  name: "SwiftLintBuildToolPlugin", package: "SwiftLintPlugins")

let package = Package(
  name: "CoxPlatform",
  platforms: [.macOS(.v26)],
  products: [
    .library(name: "CoxPlatform", targets: ["CoxPlatform"])
  ],
  dependencies: [
    .package(path: "../CoxModel"),
    .package(url: "https://github.com/SimplyDanny/SwiftLintPlugins", exact: "0.65.1"),
  ],
  targets: [
    .target(
      name: "CoxPlatform",
      dependencies: [.product(name: "CoxClient", package: "CoxModel")],
      linkerSettings: [.linkedFramework("Security")],
      plugins: [swiftLint]
    ),
    .testTarget(
      name: "CoxPlatformTests",
      dependencies: ["CoxPlatform", .product(name: "CoxClient", package: "CoxModel")],
      plugins: [swiftLint]
    ),
  ]
)
