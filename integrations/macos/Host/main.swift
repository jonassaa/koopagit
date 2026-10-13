// KoopaGit.app host: owns the Finder Sync extension and runs menu actions.
//
// The sandboxed extension cannot pass launch arguments, so it writes a menu
// item's invoke array (JSON) to a `.koopagit-invoke` file in its container and
// opens that file with this app, which runs the bundled
// `koopagit-shell invoke <args...>` and exits. `KoopaGit invoke <args...>` does
// the same from a terminal. Launched normally, it explains how to enable the
// extension and where the watched roots are configured.

import Cocoa

let bundleMacOS = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS")

/// Only request files written by our extension are honoured.
let requestDir = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent("Library/Containers/dev.koopagit.FinderSync/Data")
    .resolvingSymlinksInPath().path + "/"

/// Runs `koopagit-shell invoke <args...>` and returns its exit status.
func runInvoke(_ args: [String]) -> Int32 {
    let process = Process()
    process.executableURL = bundleMacOS.appendingPathComponent("koopagit-shell")
    process.arguments = ["invoke"] + args
    var env = ProcessInfo.processInfo.environment
    env["KOOPAGIT_PROC"] = bundleMacOS.appendingPathComponent("koopagit-proc").path
    process.environment = env
    do {
        try process.run()
        process.waitUntilExit()
        return process.terminationStatus
    } catch {
        NSLog("koopagit: \(error)")
        return 1
    }
}

let args = Array(CommandLine.arguments.dropFirst())
if let i = args.firstIndex(of: "invoke") {
    exit(runInvoke(Array(args[(i + 1)...])))
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var handledRequest = false

    // Sent before applicationDidFinishLaunching when launched to open files.
    func application(_ application: NSApplication, open urls: [URL]) {
        handledRequest = true
        for url in urls {
            let path = url.resolvingSymlinksInPath().path
            guard url.pathExtension == "koopagit-invoke", path.hasPrefix(requestDir) else {
                NSLog("koopagit: ignoring \(path)")
                continue
            }
            let data = try? Data(contentsOf: url)
            try? FileManager.default.removeItem(at: url)
            guard let data, let invoke = try? JSONSerialization.jsonObject(with: data) as? [String] else {
                NSLog("koopagit: malformed request \(path)")
                continue
            }
            _ = runInvoke(invoke)
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        if !handledRequest {
            showHelp()
        }
        NSApp.terminate(nil)
    }

    private func showHelp() {
        NSApp.setActivationPolicy(.regular)
        let alert = NSAlert()
        alert.messageText = "KoopaGit"
        alert.informativeText = """
            Enable the Finder extension in System Settings → General → Login Items & Extensions → \
            File Providers / Finder Extensions (or run: pluginkit -e use -i dev.koopagit.FinderSync).

            Badges and menus appear inside the folders listed (one per line) in \
            ~/.config/koopagit/finder-roots; your home folder is used when that file is missing.
            """
        alert.addButton(withTitle: "OK")
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }
}

let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.run()
