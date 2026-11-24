// SPDX-License-Identifier: GPL-3.0-or-later

import CXenora
import GameController

struct ControllerSnapshot: Equatable {
    let buttons: UInt32
    let x: Int16
    let y: Int16
    let leftTrigger: Float
    let rightTrigger: Float

    init(buttons: UInt32, x: Float, y: Float, leftTrigger: Float, rightTrigger: Float) {
        self.buttons = buttons
        self.x = Self.axis(x)
        self.y = Self.axis(y)
        self.leftTrigger = Self.trigger(leftTrigger)
        self.rightTrigger = Self.trigger(rightTrigger)
    }

    private static func axis(_ value: Float) -> Int16 {
        guard value.isFinite else { return 0 }
        let deadZone: Float = 0.15
        let magnitude = max(0, min(1, abs(value)) - deadZone) / (1 - deadZone)
        return Int16((magnitude * (value < 0 ? -32767 : 32767)).rounded())
    }

    private static func trigger(_ value: Float) -> Float {
        value.isFinite ? min(1, max(0, value)) : 0
    }
}

struct DetectedController: Equatable {
    let identity: ObjectIdentifier
    let name: String
    let supportsHaptics: Bool
    let snapshot: ControllerSnapshot
}

@MainActor
func pollGameController() -> DetectedController? {
    let controller = GCController.current.flatMap { $0.extendedGamepad == nil ? nil : $0 }
        ?? GCController.controllers().first { $0.extendedGamepad != nil }
    guard let controller, let pad = controller.capture().extendedGamepad else { return nil }
    var buttons: UInt32 = 0
    if pad.dpad.right.isPressed { buttons |= UInt32(XENORA_INPUT_RIGHT) }
    if pad.dpad.left.isPressed { buttons |= UInt32(XENORA_INPUT_LEFT) }
    if pad.dpad.up.isPressed { buttons |= UInt32(XENORA_INPUT_UP) }
    if pad.dpad.down.isPressed { buttons |= UInt32(XENORA_INPUT_DOWN) }
    if pad.buttonA.isPressed { buttons |= UInt32(XENORA_INPUT_ACTION_A) }
    if pad.buttonB.isPressed { buttons |= UInt32(XENORA_INPUT_ACTION_B) }
    return DetectedController(
        identity: ObjectIdentifier(controller),
        name: controller.vendorName ?? controller.productCategory,
        supportsHaptics: controller.haptics != nil,
        snapshot: ControllerSnapshot(
            buttons: buttons, x: pad.leftThumbstick.xAxis.value, y: pad.leftThumbstick.yAxis.value,
            leftTrigger: pad.leftTrigger.value, rightTrigger: pad.rightTrigger.value
        )
    )
}
