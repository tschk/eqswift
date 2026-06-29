// swift-tools-version:5.9
import PackageDescription

let profile = ProcessInfo.processInfo.environment["EQSWIFT_PROFILE"] ?? "debug"
let staticLink = ProcessInfo.processInfo.environment["EQSWIFT_STATIC"] == "1"
let rustLibDir = "../../target/\(profile)"

var rustLinkFlags = ["-L", rustLibDir]
if staticLink {
    rustLinkFlags += ["-force_load", "\(rustLibDir)/libeqswift.a"]
} else {
    rustLinkFlags += ["-leqswift"]
}

let package = Package(
    name: "EqSwift",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "EqSwift", targets: ["EqSwift"]),
    ],
    targets: [
        .systemLibrary(
            name: "eqswiftFFI",
            path: "Generated"
        ),
        .target(
            name: "EqSwift",
            dependencies: ["eqswiftFFI"],
            path: "Generated",
            sources: ["eqswift.swift"],
            linkerSettings: [
                .unsafeFlags(rustLinkFlags),
            ]
        ),
    ]
)