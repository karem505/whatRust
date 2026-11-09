import Foundation
import CoreGraphics
// usage: hwkey <keycode> [fn]   posts key down/up at HID level; fn => hardware-like arrow flags
let a = CommandLine.arguments
let code = CGKeyCode(UInt16(a[1])!)
let fn = a.count > 2 && a[2] == "fn"
let src = CGEventSource(stateID: .hidSystemState)
for down in [true, false] {
    let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)!
    if fn { e.flags = [.maskSecondaryFn, .maskNumericPad] }
    e.post(tap: .cghidEventTap)
    usleep(40000)
}
