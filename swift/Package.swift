// swift-tools-version:5.9
// Sample package for examples/demo (Rust crate `eqswift_demo`). Generate bindings with
// `cargo eqswift swift --out-dir ../../swift/Generated` from examples/demo (see README).
import Foundation
import PackageDescription

let profile = ProcessInfo.processInfo.environment["EQSWIFT_PROFILE"] ?? "debug"
let staticLink = ProcessInfo.processInfo.environment["EQSWIFT_STATIC"] == "1"
let repoRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().path
let rustLibDir = "\(repoRoot)/target/\(profile)"

var rustLinkFlags = ["-L", rustLibDir]
if staticLink {
    rustLinkFlags += ["-force_load", "\(rustLibDir)/libeqswift_demo.a"]
} else {
    rustLinkFlags += ["-leqswift_demo"]
}

let package = Package(
    name: "EqSwiftDemo",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "EqSwiftDemo", targets: ["EqSwiftDemo"]),
    ],
    targets: [
        .systemLibrary(
            name: "eqswift_demoFFI",
            path: "Generated"
        ),
        .target(
            name: "EqSwiftDemo",
            dependencies: ["eqswift_demoFFI"],
            path: "Generated",
            sources: ["eqswift_demo.swift"],
            linkerSettings: [
                .unsafeFlags(rustLinkFlags),
            ]
        ),
    ]
)
