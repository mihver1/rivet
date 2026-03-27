// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "RivetApp",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        .executableTarget(
            name: "RivetApp",
            path: "Sources"
        ),
    ]
)
