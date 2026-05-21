// swift-tools-version:5.9
import PackageDescription

/// Rust `eqswift` dylib (workspace root `../target/debug`, relative to this manifest).
let rustLibDir = "../../target/debug"

let package = Package(
    name: "EqSwift",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "EqSwift", targets: ["EqSwift"]),
    ],
    targets: [
        .systemLibrary(
            name: "eqswiftFFI",
            path: "eqswiftFFI"
        ),
        .target(
            name: "EqSwift",
            dependencies: ["eqswiftFFI"],
            path: "Sources/EqSwift",
            linkerSettings: [
                .unsafeFlags([
                    "-L", rustLibDir, "-leqswift",
                ]),
            ]
        ),
    ]
)
