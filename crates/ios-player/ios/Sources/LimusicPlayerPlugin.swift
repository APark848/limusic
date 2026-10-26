import AVFoundation
import Foundation
import Tauri

@_silgen_name("limusic_ios_player_emit")
private func emitToRust(_ kind: UInt32, _ value: Double, _ message: UnsafePointer<CChar>?)

private enum EventKind {
  static let position: UInt32 = 0
  static let duration: UInt32 = 1
  static let playing: UInt32 = 2
  static let paused: UInt32 = 3
  static let ended: UInt32 = 4
  static let failed: UInt32 = 5
  static let error: UInt32 = 6
  static let diagnostic: UInt32 = 7
}

private struct LoadArgs: Decodable {
  let url: String
  let headers: [String: String]
  let start: Double?
}

private struct SeekArgs: Decodable {
  let position: Double
}

private struct VolumeArgs: Decodable {
  let volume: Int
}

private struct LoopArgs: Decodable {
  let enabled: Bool
}

final class LimusicPlayerPlugin: Plugin {
  private let player = AVPlayer()
  private var periodicObserver: Any?
  private var statusObservation: NSKeyValueObservation?
  private var timeControlObservation: NSKeyValueObservation?
  private var endedObserver: NSObjectProtocol?
  private var failedObserver: NSObjectProtocol?
  private var loopsCurrentItem = false
  private var failureEmitted = false

  deinit {
    removeObservers()
  }

  private func emit(_ kind: UInt32, value: Double = 0, message: String? = nil) {
    if let message {
      message.withCString { emitToRust(kind, value, $0) }
    } else {
      emitToRust(kind, value, nil)
    }
  }

  private func resolveOnMain(_ invoke: Invoke, action: @escaping () throws -> Void) {
    let work = {
      do {
        try action()
        invoke.resolve()
      } catch {
        invoke.reject(error.localizedDescription)
      }
    }
    if Thread.isMainThread {
      work()
    } else {
      DispatchQueue.main.async(execute: work)
    }
  }

  private func removeObservers() {
    if let observer = periodicObserver {
      player.removeTimeObserver(observer)
      periodicObserver = nil
    }
    statusObservation?.invalidate()
    statusObservation = nil
    timeControlObservation?.invalidate()
    timeControlObservation = nil
    if let observer = endedObserver {
      NotificationCenter.default.removeObserver(observer)
      endedObserver = nil
    }
    if let observer = failedObserver {
      NotificationCenter.default.removeObserver(observer)
      failedObserver = nil
    }
  }

  private func emitFailure(_ message: String) {
    guard !failureEmitted else { return }
    failureEmitted = true
    emit(EventKind.failed, message: message)
  }

  private func diagnostic(_ stage: String) {
    emit(EventKind.diagnostic, message: stage)
  }

  private func safeFailure(_ error: Error?, fallback: String) -> String {
    guard let error else { return fallback }
    let nsError = error as NSError
    return "\(fallback) (domain=\(nsError.domain), code=\(nsError.code))"
  }

  private func cookies(from header: String, for url: URL) throws -> [HTTPCookie] {
    guard let host = url.host else {
      throw NSError(
        domain: "LimusicPlayer",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "Stream URL has no host for its Cookie header"])
    }

    return try header.split(separator: ";").map { part in
      let pair = part.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
      guard pair.count == 2 else {
        throw NSError(
          domain: "LimusicPlayer",
          code: 3,
          userInfo: [NSLocalizedDescriptionKey: "Invalid Cookie header in stream request"])
      }
      let name = pair[0].trimmingCharacters(in: .whitespaces)
      let value = String(pair[1]).trimmingCharacters(in: .whitespaces)
      var properties: [HTTPCookiePropertyKey: Any] = [
        .domain: host,
        .path: "/",
        .name: name,
        .value: value,
      ]
      if url.scheme?.lowercased() == "https" {
        properties[.secure] = "TRUE"
      }
      guard !name.isEmpty, let cookie = HTTPCookie(properties: properties) else {
        throw NSError(
          domain: "LimusicPlayer",
          code: 4,
          userInfo: [NSLocalizedDescriptionKey: "Invalid Cookie header in stream request"])
      }
      return cookie
    }
  }

  private func makeAsset(url: URL, headers: [String: String]) throws -> AVURLAsset {
    var options: [String: Any] = [:]
    for (name, value) in headers {
      switch name.lowercased() {
      case "user-agent":
        options[AVURLAssetHTTPUserAgentKey] = value
      case "cookie":
        options[AVURLAssetHTTPCookiesKey] = try cookies(from: value, for: url)
      default:
        throw NSError(
          domain: "LimusicPlayer",
          code: 5,
          userInfo: [
            NSLocalizedDescriptionKey:
              "The iOS player does not support the required HTTP header '\(name)'"
          ])
      }
    }
    return AVURLAsset(url: url, options: options)
  }

  private func observe(_ item: AVPlayerItem) {
    periodicObserver = player.addPeriodicTimeObserver(
      forInterval: CMTime(seconds: 0.25, preferredTimescale: 600),
      queue: .main
    ) { [weak self] time in
      let seconds = time.seconds
      if seconds.isFinite {
        self?.emit(EventKind.position, value: max(0, seconds))
      }
    }

    statusObservation = item.observe(\.status, options: [.initial, .new]) {
      [weak self, weak item] observed, _ in
      guard let self else { return }
      switch observed.status {
      case .readyToPlay:
        self.diagnostic("item-ready-to-play")
        let seconds = observed.duration.seconds
        if seconds.isFinite && seconds >= 0 {
          self.emit(EventKind.duration, value: seconds)
        }
      case .failed:
        self.diagnostic("item-failed")
        self.emitFailure(self.safeFailure(item?.error, fallback: "AVPlayer could not load this track"))
      case .unknown:
        self.diagnostic("item-status-unknown")
      @unknown default:
        self.emit(EventKind.error, message: "AVPlayer reported an unknown item status")
      }
    }

    timeControlObservation = player.observe(\.timeControlStatus, options: [.initial, .new]) {
      [weak self] observed, _ in
      guard let self else { return }
      switch observed.timeControlStatus {
      case .playing:
        self.diagnostic("time-control-playing")
        self.emit(EventKind.playing, value: 1)
        self.emit(EventKind.paused, value: 0)
      case .paused:
        self.diagnostic("time-control-paused")
        self.emit(EventKind.playing, value: 0)
        self.emit(EventKind.paused, value: 1)
      case .waitingToPlayAtSpecifiedRate:
        self.diagnostic("time-control-waiting")
      @unknown default:
        self.diagnostic("time-control-unknown")
      }
    }

    endedObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemDidPlayToEndTime,
      object: item,
      queue: .main
    ) { [weak self] _ in
      guard let self else { return }
      if self.loopsCurrentItem {
        self.player.seek(to: .zero)
        self.player.play()
      } else {
        self.emit(EventKind.playing, value: 0)
        self.emit(EventKind.ended)
      }
    }

    failedObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemFailedToPlayToEndTime,
      object: item,
      queue: .main
    ) { [weak self] notification in
      guard let self else { return }
      let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
      self.emitFailure(self.safeFailure(error, fallback: "AVPlayer failed before the track ended"))
    }

    DispatchQueue.main.asyncAfter(deadline: .now() + 20) { [weak self, weak item] in
      guard let self, let item, item === self.player.currentItem, item.status == .unknown else {
        return
      }
      self.diagnostic("item-status-timeout")
      self.emitFailure("AVPlayer could not prepare this track")
    }
  }

  @objc public func load(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(LoadArgs.self)
    guard let url = URL(string: args.url), let scheme = url.scheme?.lowercased() else {
      invoke.reject("Playback requires a valid stream URL")
      return
    }
    #if DEBUG
      let supportedSchemes = ["http", "https", "file"]
    #else
      let supportedSchemes = ["http", "https"]
    #endif
    guard supportedSchemes.contains(scheme) else {
      invoke.reject("Playback requires an HTTP or HTTPS stream URL")
      return
    }
    let asset = try makeAsset(url: url, headers: args.headers)
    let start = args.start

    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      self.removeObservers()
      self.failureEmitted = false
      self.diagnostic("load-received")
      let item = AVPlayerItem(asset: asset)
      self.player.replaceCurrentItem(with: item)
      self.diagnostic("item-installed")
      self.observe(item)
      if let start, start > 0 {
        self.player.seek(to: CMTime(seconds: start, preferredTimescale: 600))
      }
      self.emit(EventKind.position, value: start ?? 0)
    }
  }

  @objc public func play(_ invoke: Invoke) {
    resolveOnMain(invoke) { [weak self] in
      guard let self, self.player.currentItem != nil else {
        throw NSError(
          domain: "LimusicPlayer",
          code: 6,
          userInfo: [NSLocalizedDescriptionKey: "No track is loaded"])
      }
      let session = AVAudioSession.sharedInstance()
      try session.setCategory(.playback, mode: .default)
      try session.setActive(true)
      self.diagnostic("audio-session-active")
      self.player.play()
      self.diagnostic("play-requested")
    }
  }

  @objc public func pause(_ invoke: Invoke) {
    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      self.player.pause()
      self.emit(EventKind.playing, value: 0)
      self.emit(EventKind.paused, value: 1)
    }
  }

  @objc public func stop(_ invoke: Invoke) {
    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      self.player.pause()
      self.removeObservers()
      self.player.replaceCurrentItem(with: nil)
      self.emit(EventKind.position, value: 0)
      self.emit(EventKind.playing, value: 0)
    }
  }

  @objc public func seek(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SeekArgs.self)
    resolveOnMain(invoke) { [weak self] in
      guard let self, self.player.currentItem != nil else {
        throw NSError(
          domain: "LimusicPlayer",
          code: 7,
          userInfo: [NSLocalizedDescriptionKey: "No track is loaded"])
      }
      self.player.seek(
        to: CMTime(seconds: args.position, preferredTimescale: 600),
        toleranceBefore: .zero,
        toleranceAfter: .zero)
    }
  }

  @objc public func setVolume(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(VolumeArgs.self)
    resolveOnMain(invoke) { [weak self] in
      self?.player.volume = Float(args.volume) / 100
    }
  }

  @objc public func setLoop(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(LoopArgs.self)
    resolveOnMain(invoke) { [weak self] in
      self?.loopsCurrentItem = args.enabled
    }
  }
}

@_cdecl("init_plugin_limusic_player")
func initPlugin() -> Plugin {
  LimusicPlayerPlugin()
}
