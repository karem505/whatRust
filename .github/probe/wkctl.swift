import Cocoa
import WebKit
// Minimal native WKWebView app: control for the arrow-key probe.
// args: url [menu]
class Parent: NSView {
    override func keyDown(with event: NSEvent) { NSApp.mainMenu?.performKeyEquivalent(with: event) }
}
class SubWV: WKWebView {
    var pkeNo = false
    override func performKeyEquivalent(with event: NSEvent) -> Bool { pkeNo ? false : super.performKeyEquivalent(with: event) }
}
class TaoLikeWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
    override func sendEvent(_ e: NSEvent) { super.sendEvent(e) }
}
class D: NSObject, NSApplicationDelegate {
    var w: NSWindow!
    var wv: WKWebView!
    func applicationDidFinishLaunching(_ n: Notification) {
        if CommandLine.arguments.count > 2 && CommandLine.arguments[2] == "menu" {
            let main = NSMenu()
            let appItem = NSMenuItem(); main.addItem(appItem)
            let appMenu = NSMenu(); appItem.submenu = appMenu
            appMenu.addItem(withTitle: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
            let editItem = NSMenuItem(); main.addItem(editItem)
            let edit = NSMenu(title: "Edit"); editItem.submenu = edit
            edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
            edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
            edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
            edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
            NSApp.mainMenu = main
        }
        let st: NSWindow.StyleMask = [.titled, .closable, .resizable]
        w = (CommandLine.arguments.count > 3 && CommandLine.arguments[3].contains("taowin"))
            ? TaoLikeWindow(contentRect: NSRect(x: 100, y: 100, width: 900, height: 600), styleMask: st, backing: .buffered, defer: false)
            : NSWindow(contentRect: NSRect(x: 100, y: 100, width: 900, height: 600), styleMask: st, backing: .buffered, defer: false)
        let flags = CommandLine.arguments.count > 3 ? CommandLine.arguments[3] : ""
        let cfg = WKWebViewConfiguration()
        let prefs = cfg.preferences
        if flags.contains("prefs") {
            for k in ["tabFocusesLinks", "allowsPictureInPictureMediaPlayback", "fullScreenEnabled"] { prefs.setValue(true, forKey: k) }
            prefs.setValue(1, forKey: "inactiveSchedulingPolicy")
        }
        if flags.contains("dev") { prefs.setValue(true, forKey: "developerExtrasEnabled") }
        let sub = SubWV(frame: w.contentView!.bounds, configuration: cfg)
        sub.pkeNo = flags.contains("pkeno")
        wv = flags.contains("sub") || flags.contains("pkeno") ? sub : WKWebView(frame: w.contentView!.bounds, configuration: cfg)
        if flags.contains("dev") { wv.isInspectable = true }
        if flags.contains("ua") { wv.customUserAgent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15" }
        wv.autoresizingMask = [.width, .height]
        if flags.contains("parent") {
            let p = Parent(frame: w.contentView!.bounds)
            p.autoresizingMask = [.width, .height]
            w.contentView = p
        }
        w.contentView!.addSubview(wv)
        wv.load(URLRequest(url: URL(string: CommandLine.arguments[1])!))
        w.makeKeyAndOrderFront(nil)
        w.makeFirstResponder(wv)
        NSApp.activate(ignoringOtherApps: true)
    }
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let d = D()
app.delegate = d
app.run()
