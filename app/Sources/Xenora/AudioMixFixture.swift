// SPDX-License-Identifier: GPL-3.0-or-later

import AVFoundation
import Foundation

struct AudioMixMeasurement {
    let frames: Int
    let mean: Double
    let toneAmplitude: Double
}

enum AudioMixFixtureError: Error {
    case bufferAllocation
    case renderFailure(AVAudioEngineManualRenderingStatus)
    case shortRender
}

func measureAudioMixFixture() throws -> AudioMixMeasurement {
    let outputRate = 48_000.0
    let toneRate = 44_100.0
    let toneFrequency = 1_000.0
    let blockSize: AVAudioFrameCount = 480
    let totalFrames = 4_800
    guard let outputFormat = AVAudioFormat(standardFormatWithSampleRate: outputRate, channels: 1),
          let toneFormat = AVAudioFormat(standardFormatWithSampleRate: toneRate, channels: 1),
          let constant = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: 4_800),
          let tone = AVAudioPCMBuffer(pcmFormat: toneFormat, frameCapacity: 4_410),
          let output = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: blockSize),
          let constantSamples = constant.floatChannelData?[0],
          let toneSamples = tone.floatChannelData?[0],
          let outputSamples = output.floatChannelData?[0] else {
        throw AudioMixFixtureError.bufferAllocation
    }
    constant.frameLength = constant.frameCapacity
    tone.frameLength = tone.frameCapacity
    for index in 0..<Int(constant.frameLength) {
        constantSamples[index] = 0.125
    }
    for index in 0..<Int(tone.frameLength) {
        toneSamples[index] = Float(0.25 * sin(2 * .pi * toneFrequency * Double(index) / toneRate))
    }

    let engine = AVAudioEngine()
    let constantPlayer = AVAudioPlayerNode()
    let tonePlayer = AVAudioPlayerNode()
    engine.attach(constantPlayer)
    engine.attach(tonePlayer)
    engine.connect(constantPlayer, to: engine.mainMixerNode, format: outputFormat)
    engine.connect(tonePlayer, to: engine.mainMixerNode, format: toneFormat)
    try engine.enableManualRenderingMode(.offline, format: outputFormat, maximumFrameCount: blockSize)
    defer {
        constantPlayer.stop()
        tonePlayer.stop()
        engine.stop()
        engine.disableManualRenderingMode()
    }
    try engine.start()
    constantPlayer.scheduleBuffer(constant)
    tonePlayer.scheduleBuffer(tone)
    constantPlayer.play()
    tonePlayer.play()

    var mean = 0.0
    var sine = 0.0
    var cosine = 0.0
    var measured = 0
    for first in stride(from: 0, to: totalFrames, by: Int(blockSize)) {
        let status = try engine.renderOffline(blockSize, to: output)
        guard status == .success else { throw AudioMixFixtureError.renderFailure(status) }
        guard output.frameLength == blockSize else { throw AudioMixFixtureError.shortRender }
        // Exclude the leading and trailing conversion transients.
        if first == 0 || first == totalFrames - Int(blockSize) { continue }
        for offset in 0..<Int(blockSize) {
            let sample = Double(outputSamples[offset])
            let phase = 2 * Double.pi * toneFrequency * Double(first + offset) / outputRate
            mean += sample
            sine += sample * sin(phase)
            cosine += sample * cos(phase)
            measured += 1
        }
    }
    let count = Double(measured)
    return AudioMixMeasurement(frames: totalFrames, mean: mean / count, toneAmplitude: 2 * hypot(sine, cosine) / count)
}
