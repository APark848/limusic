// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "limusic-ios-player",
  platforms: [
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "limusic-ios-player",
      type: .static,
      targets: ["limusic-ios-player"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "limusic-ios-player",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
