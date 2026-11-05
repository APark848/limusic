import AVFoundation
import Foundation
import Network
import Tauri

@_silgen_name("limusic_ios_player_emit")
private func emitToRust(_ kind: UInt32, _ value: Double, _ message: UnsafePointer<CChar>?, _ epoch: UInt64)

private enum EventKind {
  static let position: UInt32 = 0
  static let duration: UInt32 = 1
  static let playing: UInt32 = 2
  static let paused: UInt32 = 3
  static let ended: UInt32 = 4
  static let failed: UInt32 = 5
  static let error: UInt32 = 6
  static let diagnostic: UInt32 = 7
  static let buffering: UInt32 = 8
  static let networkRestored: UInt32 = 9
}

private struct LoadArgs: Decodable {
  let epoch: UInt64
  let url: String
  let headers: [String: String]
  let start: Double?
  let expectedDuration: Double?
}

private struct PrepareArgs: Decodable {
  let url: String
  let headers: [String: String]
  let expectedDuration: Double?
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
  private var eventEpoch: UInt64 = 0
  private let player = AVPlayer()
  private var periodicObserver: Any?
  private var statusObservation: NSKeyValueObservation?
  private var timeControlObservation: NSKeyValueObservation?
  private var endedObserver: NSObjectProtocol?
  private var failedObserver: NSObjectProtocol?
  private var loopsCurrentItem = false
  private var failureEmitted = false
  private var wantsPlayback = false
  private var expectedDuration: Double?
  private var usesExpectedDuration = false
  private var endedEmitted = false
  private var authoritativeBoundaryHandled = false
  private var loadStartedAt = ProcessInfo.processInfo.systemUptime
  private var firstProgressEmitted = false
  private var hasPlayed = false
  private var preparedAsset: AVURLAsset?
  private var preparedURL: String?
  private var preparedHeaders: [String: String]?
  private var preparedExpectedDuration: Double?
  private let networkMonitor = NWPathMonitor()
  private var networkMonitoringStarted = false
  private var wasOffline = false
  private var resumeStartedAt: Double?
  private var progressBaseline: Double = 0
  private var transportStatus: AVPlayer.TimeControlStatus = .paused

  deinit {
    networkMonitor.cancel()
    removeObservers()
  }

  private func emit(_ kind: UInt32, value: Double = 0, message: String? = nil) {
    if let message {
      message.withCString { emitToRust(kind, value, $0, eventEpoch) }
    } else {
      emitToRust(kind, value, nil, eventEpoch)
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
    emit(
      EventKind.diagnostic,
      value: max(0, (ProcessInfo.processInfo.systemUptime - loadStartedAt) * 1000),
      message: stage)
  }

  private func monitorConnectivity() {
    guard !networkMonitoringStarted else { return }
    networkMonitoringStarted = true
    networkMonitor.pathUpdateHandler = { [weak self] path in
      DispatchQueue.main.async {
        guard let self else { return }
        let reconnected = self.wasOffline && path.status == .satisfied
        self.wasOffline = path.status != .satisfied
        if reconnected { self.emit(EventKind.networkRestored) }
        // A path change is not an item failure. Keep the item, timestamp and pause intent.
        if reconnected, self.wantsPlayback, let item = self.player.currentItem,
          item.status == .readyToPlay, self.transportStatus != .playing {
          self.startPlaybackIfReady(item, stage: "network-restored-resume")
        }
      }
    }
    networkMonitor.start(queue: DispatchQueue(label: "LimusicConnectivity"))
  }

  private func bufferedAhead(_ item: AVPlayerItem) -> Double {
    let position = player.currentTime().seconds
    guard position.isFinite else { return 0 }
    return item.loadedTimeRanges.reduce(0) { best, value in
      let range = value.timeRangeValue
      let start = range.start.seconds
      let end = CMTimeRangeGetEnd(range).seconds
      return start <= position && end > position ? max(best, end - position) : best
    }
  }

  private func clearPreparedAsset() {
    preparedAsset = nil
    preparedURL = nil
    preparedHeaders = nil
    preparedExpectedDuration = nil
  }

  private func finishCurrentItem() {
    guard !endedEmitted else { return }
    endedEmitted = true
    wantsPlayback = false
    player.pause()
    emit(EventKind.playing, value: 0)
    emit(EventKind.ended)
  }

  private func reachAuthoritativeEnd(_ item: AVPlayerItem) {
    guard !authoritativeBoundaryHandled else { return }
    authoritativeBoundaryHandled = true
    if loopsCurrentItem {
      player.seek(to: .zero) { [weak self, weak item] _ in
        guard let self, let item, item === self.player.currentItem else { return }
        self.authoritativeBoundaryHandled = false
        self.startPlaybackIfReady(item, stage: "authoritative-duration-loop-restart")
      }
    } else {
      finishCurrentItem()
    }
  }

  private func startPlaybackIfReady(_ item: AVPlayerItem, stage: String) {
    guard item === player.currentItem, wantsPlayback else { return }
    guard item.status == .readyToPlay else {
      diagnostic("play-deferred-until-ready")
      return
    }
    player.play()
    diagnostic(stage)
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
    ) { [weak self, weak item] time in
      guard let self, let item, item === self.player.currentItem else { return }
      let seconds = time.seconds
      if seconds.isFinite {
        self.emit(EventKind.position, value: max(0, seconds))
        if self.transportStatus == .playing, seconds > self.progressBaseline + 0.01,
          !self.firstProgressEmitted {
          self.firstProgressEmitted = true
          self.diagnostic("first-playback-progress")
        }
        if let started = self.resumeStartedAt, self.transportStatus == .playing,
          seconds > self.progressBaseline + 0.01 {
          self.emit(EventKind.diagnostic, value: (ProcessInfo.processInfo.systemUptime - started) * 1000,
            message: "resume-first-progress")
          self.resumeStartedAt = nil
        }
        if self.usesExpectedDuration,
          let expected = self.expectedDuration,
          seconds >= expected
        {
          self.diagnostic("authoritative-duration-ended")
          if let item = self.player.currentItem {
            self.reachAuthoritativeEnd(item)
          }
        }
      }
    }

    statusObservation = item.observe(\.status, options: [.initial, .new]) {
      [weak self, weak item] observed, _ in
      DispatchQueue.main.async { [weak self, weak item] in
        guard let self else { return }
        guard observed === self.player.currentItem else { return }
        switch observed.status {
        case .readyToPlay:
          self.diagnostic("item-ready-to-play")
          let observedSeconds = observed.duration.seconds
          if observedSeconds.isFinite && observedSeconds >= 0 {
            if let expected = self.expectedDuration {
              let tolerance = max(10, expected * 0.1)
              self.usesExpectedDuration = abs(observedSeconds - expected) > tolerance
            }
            let reported = self.usesExpectedDuration ? self.expectedDuration! : observedSeconds
            self.emit(EventKind.duration, value: reported)
            if self.usesExpectedDuration {
              self.diagnostic("duration-authoritative-fallback")
            }
          }
          self.startPlaybackIfReady(observed, stage: "ready-autoplay")
        case .failed:
          self.wantsPlayback = false
          self.diagnostic("item-failed")
          self.emitFailure(self.safeFailure(item?.error, fallback: "AVPlayer could not load this track"))
        case .unknown:
          self.diagnostic("item-status-unknown")
        @unknown default:
          self.wantsPlayback = false
          self.emit(EventKind.error, message: "AVPlayer reported an unknown item status")
        }
      }
    }
    self.diagnostic("status-observer-installed")

    // Read native enum properties after the KVO callback returns, outside AVFoundation's
    // notification stack. KVO's generic change.newValue may be nil for these ObjC enum values.
    timeControlObservation = player.observe(\.timeControlStatus, options: [.new]) {
      [weak self, weak item] observed, _ in
      DispatchQueue.main.async { [weak self, weak item] in
        guard let self, let item, item === self.player.currentItem else { return }
        let status = observed.timeControlStatus
        if status == .playing && !self.wantsPlayback { return }
        self.transportStatus = status
        switch status {
        case .playing:
          self.hasPlayed = true
          self.diagnostic("time-control-playing")
          self.emit(EventKind.playing, value: 1)
          self.emit(EventKind.paused, value: 0)
        case .paused:
          if self.wantsPlayback {
            self.diagnostic("time-control-paused-with-playback-intent")
            self.emit(EventKind.buffering)
          } else {
            self.diagnostic("time-control-paused")
            self.emit(EventKind.playing, value: 0)
            self.emit(EventKind.paused, value: 1)
          }
        case .waitingToPlayAtSpecifiedRate:
          let reason: String
          switch observed.reasonForWaitingToPlay {
          case .toMinimizeStalls?: reason = "minimize-stalls"
          case .evaluatingBufferingRate?: reason = "evaluating-buffer"
          case .noItemToPlay?: reason = "no-item"
          default: reason = "unspecified"
          }
          self.diagnostic("time-control-waiting-\(reason)")
          if self.wantsPlayback { self.emit(EventKind.buffering) }
        @unknown default:
          self.diagnostic("time-control-unknown")
        }
      }
    }
    self.diagnostic("transport-observer-installed")

    endedObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemDidPlayToEndTime,
      object: item,
      queue: .main
    ) { [weak self] _ in
      guard let self else { return }
      if self.loopsCurrentItem {
        self.player.seek(to: .zero)
        self.startPlaybackIfReady(item, stage: "loop-restart")
      } else {
        self.finishCurrentItem()
      }
    }

    failedObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemFailedToPlayToEndTime,
      object: item,
      queue: .main
    ) { [weak self] notification in
      guard let self else { return }
      guard item === self.player.currentItem else { return }
      let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
      if item.status != .failed, let error = error as NSError?, error.domain == NSURLErrorDomain {
        // A recoverable transport interruption must not trigger Rust's replace/re-resolve path.
        self.diagnostic("network-interrupted-item-retained")
        if self.wantsPlayback { self.emit(EventKind.buffering) }
        return
      }
      self.wantsPlayback = false
      self.emitFailure(self.safeFailure(error, fallback: "AVPlayer failed before the track ended"))
    }

    DispatchQueue.main.asyncAfter(deadline: .now() + 20) { [weak self, weak item] in
      guard let self, let item, item === self.player.currentItem, item.status == .unknown else {
        return
      }
      self.wantsPlayback = false
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
    let start = args.start

    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      let canUsePrepared = self.preparedAsset != nil && self.preparedURL == args.url
        && self.preparedHeaders == args.headers
        && self.preparedExpectedDuration == args.expectedDuration
      let asset: AVURLAsset
      if canUsePrepared {
        asset = self.preparedAsset!
      } else {
        asset = try self.makeAsset(url: url, headers: args.headers)
      }
      self.monitorConnectivity()
      self.removeObservers()
      self.eventEpoch = args.epoch
      self.transportStatus = .paused
      self.wantsPlayback = false
      self.failureEmitted = false
      self.expectedDuration = args.expectedDuration
      self.usesExpectedDuration = false
      self.endedEmitted = false
      self.authoritativeBoundaryHandled = false
      self.loadStartedAt = ProcessInfo.processInfo.systemUptime
      self.firstProgressEmitted = false
      self.hasPlayed = false
      self.progressBaseline = start ?? 0
      self.resumeStartedAt = nil
      self.diagnostic("load-received")
      self.diagnostic(canUsePrepared ? "prepared-asset-consumed" : "fresh-asset-installed")
      self.clearPreparedAsset()
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

  @objc public func prepare(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PrepareArgs.self)
    guard let url = URL(string: args.url), let scheme = url.scheme?.lowercased(),
      ["http", "https"].contains(scheme)
    else {
      invoke.reject("Playback preparation requires an HTTP or HTTPS stream URL")
      return
    }
    let asset = try makeAsset(url: url, headers: args.headers)
    let startedAt = ProcessInfo.processInfo.systemUptime
    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      self.preparedAsset = asset
      self.preparedURL = args.url
      self.preparedHeaders = args.headers
      self.preparedExpectedDuration = args.expectedDuration
      self.emit(EventKind.diagnostic, message: "prepare-received")
      asset.loadValuesAsynchronously(forKeys: ["playable", "duration"]) { [weak self, weak asset] in
        DispatchQueue.main.async {
          guard let self, let asset, let preparedAsset = self.preparedAsset,
            asset === preparedAsset
          else { return }
          var error: NSError?
          let status = asset.statusOfValue(forKey: "playable", error: &error)
          let elapsed = max(0, (ProcessInfo.processInfo.systemUptime - startedAt) * 1000)
          switch status {
          case .loaded:
            self.emit(EventKind.diagnostic, value: elapsed, message: "prepared-asset-ready")
          case .failed, .cancelled:
            self.emit(EventKind.diagnostic, value: elapsed, message: "prepared-asset-failed")
            self.clearPreparedAsset()
          default:
            self.emit(EventKind.diagnostic, value: elapsed, message: "prepared-asset-incomplete")
          }
        }
      }
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
      self.wantsPlayback = true
      self.emit(EventKind.paused, value: 0)
      self.emit(EventKind.buffering)
      if let item = self.player.currentItem {
        if item.status == .failed {
          self.wantsPlayback = false
          throw NSError(
            domain: "LimusicPlayer",
            code: 8,
            userInfo: [
              NSLocalizedDescriptionKey:
                self.safeFailure(item.error, fallback: "AVPlayer could not play this track")
            ])
        }
        if self.hasPlayed {
          self.resumeStartedAt = ProcessInfo.processInfo.systemUptime
          self.progressBaseline = self.player.currentTime().seconds
        }
        if self.hasPlayed, item.status == .readyToPlay, self.bufferedAhead(item) >= 0.25 {
          // A resume explicitly asks to use existing bytes. Keep automatic stall management for
          // ordinary startup and subsequent underruns; do not lower the global buffer target.
          self.player.playImmediately(atRate: 1)
          self.diagnostic("resume-buffered-item")
        } else {
          self.startPlaybackIfReady(item, stage: "play-requested")
        }
      }
    }
  }

  @objc public func pause(_ invoke: Invoke) {
    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      self.wantsPlayback = false
      self.player.pause()
      self.transportStatus = .paused
      self.emit(EventKind.playing, value: 0)
      self.emit(EventKind.paused, value: 1)
    }
  }

  @objc public func stop(_ invoke: Invoke) {
    resolveOnMain(invoke) { [weak self] in
      guard let self else { return }
      self.monitorConnectivity()
      self.wantsPlayback = false
      self.player.pause()
      self.transportStatus = .paused
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

  #if DEBUG
  @objc public func snapshot(_ invoke: Invoke) {
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      let ready = self.player.currentItem?.status == .readyToPlay
      let position = ready ? self.player.currentTime().seconds : 0
      invoke.resolve([
        "epoch": self.eventEpoch,
        "position": position.isFinite ? position : 0,
        "bufferedAhead": self.player.currentItem.flatMap {
          $0.status == .readyToPlay ? self.bufferedAhead($0) : nil
        } ?? 0,
        "wantsPlayback": self.wantsPlayback,
        "playing": self.transportStatus == .playing,
      ])
    }
  }
  #endif
}

@_cdecl("init_plugin_limusic_player")
func initPlugin() -> Plugin {
  LimusicPlayerPlugin()
}
