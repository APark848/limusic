// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "ios-player",
  platforms: [
    .iOS(.v16),
  ],
  products: [
    .library(
      name: "ios-player",
      type: .static,
      targets: ["ios-player"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "ios-player",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
