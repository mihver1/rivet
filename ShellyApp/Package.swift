// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ShellyApp",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        .executableTarget(
            name: "ShellyApp",
            path: "Sources"
        ),
    ]
)
