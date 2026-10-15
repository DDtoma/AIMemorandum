// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "AIMemorandum",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "AIMemorandum", targets: ["AIMemorandum"]),
               .executable(name: "memorandum-smoke", targets: ["Smoke"]),
               .executable(name: "memorandum-tests", targets: ["CoreTests"]),
               .library(name: "MemorandumCore", targets: ["MemorandumCore"])],
    targets: [
        .systemLibrary(name: "CSQLite"),
        .target(name: "MemorandumCore", dependencies: ["CSQLite"]),
        .executableTarget(name: "AIMemorandum", dependencies: ["MemorandumCore"]),
        .executableTarget(name: "Smoke", dependencies: ["MemorandumCore"]),
        .executableTarget(name: "CoreTests", dependencies: ["MemorandumCore"], path: "Tests/MemorandumCoreTests")
    ],
    swiftLanguageModes: [.v5]
)
