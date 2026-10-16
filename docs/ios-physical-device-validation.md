# Physical iPhone validation

This is the follow-up to the simulator playback checks. It intentionally covers only foreground
playback on one development device; it does not add background audio, lock-screen controls,
AirPlay, CarPlay, or distribution.

## Requirements

- A Mac with a supported Xcode release. Tauri's iOS commands are macOS-only, and building for a
  physical iPhone requires the iOS SDK and signing tools supplied by Xcode.
- An Apple Account added to Xcode and a signing team selected for the Limusic app target. iOS
  requires a development signature and provisioning profile even for a locally installed build.
- An iPhone 14, a data-capable USB cable (recommended for first pairing), and Developer Mode
  enabled on the phone.

A free Apple Account **is sufficient** for this foreground playback test on a personally owned
iPhone. Xcode calls this a Personal Team and manages its development certificate, device
registration, App ID, and provisioning profile. Apple's current Personal Team limits include a
seven-day provisioning lifetime, up to three registered devices, up to ten App IDs, and up to
three installed apps per device, so the app must be rebuilt periodically.

Paid Apple Developer Program membership is not required for the capability planned here: ordinary
foreground network audio playback. It becomes required for TestFlight/App Store distribution and
for Apple services or entitlements that Personal Teams cannot provision. If background audio is
considered in a later task, validate its entitlement against Apple's then-current capability table
before implementation; this task does not enable it.

Authoritative references:

- [Apple developer account overview](https://developer.apple.com/help/account/basics/about-your-developer-account)
- [Apple iOS capability availability](https://developer.apple.com/help/account/reference/supported-capabilities-ios)
- [Apple Developer Mode instructions](https://developer.apple.com/documentation/xcode/enabling-developer-mode-on-a-device)
- [Tauri mobile development](https://v2.tauri.app/develop/)
- [Tauri iOS code signing](https://v2.tauri.app/distribute/sign/ios/)

## iPhone 14 procedure

1. On the Mac, install Xcode, open it once to install requested components, and accept its license.
   Install this repository's Rust, Node/pnpm, CocoaPods, and Tauri CLI prerequisites.
2. Connect the unlocked iPhone 14 to the Mac by USB. Tap **Trust** on the iPhone if prompted and
   enter the device passcode. In Xcode, open **Window > Devices and Simulators** and wait for the
   phone to finish pairing.
3. On the iPhone, open **Settings > Privacy & Security > Developer Mode**, turn it on, restart, and
   confirm Developer Mode after restart. The option appears after pairing has begun.
4. In Xcode **Settings > Accounts**, add the Apple Account. A free account appears as a Personal
   Team.
5. From the repository root run `cargo tauri ios init` if `src-tauri/gen/apple` does not exist,
   then run `cargo tauri ios dev --open --host`. Keep that Tauri command running while Xcode is
   open; it supplies the physical-device development host.
6. In the generated Xcode project select the Limusic app target, open **Signing & Capabilities**,
   enable automatic signing, choose the Personal Team (or enrolled team), and use a unique bundle
   identifier if Xcode reports that `com.limusic.desktop` is unavailable to that team.
7. Select the paired iPhone 14 as the run destination and press **Run**. Resolve any first-use
   signing prompt in Xcode. If iOS asks whether to trust the developer, follow the on-device prompt
   and rerun.
8. With the phone volume audible and Silent Mode behavior understood, interactively select a
   public song in Limusic. Confirm that playback starts, audio is audible through the intended
   output, position advances, pause/resume and seek work, and selecting a second song replaces the
   first. Repeat once on Wi-Fi and once on cellular if cellular behavior is in scope.
9. Record the iOS version, Xcode version, device model, selected video ID (not a signed stream URL),
   resolver client, and pass/fail observations. Never capture cookies or the resolved URL's query
   string in logs or screenshots.

This procedure is the first proof of audible physical-device output and real interactive song
selection. The simulator remote-stream test does not prove either of those properties.
