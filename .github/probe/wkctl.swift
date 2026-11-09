import Cocoa
import WebKit
// Minimal native WKWebView app: control for the arrow-key probe.
// args: url [menu]
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
        w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 900, height: 600),
                     styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        wv = WKWebView(frame: w.contentView!.bounds)
        wv.autoresizingMask = [.width, .height]
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
