// KoopaGit Finder Sync extension.
//
// Thin by design (see CLAUDE.md): statuses and menus come from the bundled
// `koopagit-shell` helper; clicking a menu item asks the host app to run
// `koopagit-shell invoke ...`, so dialogs run outside Finder and outside the
// extension sandbox.

import Cocoa
import FinderSync

/// Overlay ids printed by `koopagit-shell status`, with badge colour and glyph
/// (same palette as the Nautilus emblems).
private let overlays: [(id: String, color: NSColor, glyph: String, label: String)] = [
    ("conflict", NSColor(srgbRed: 0.84, green: 0.23, blue: 0.29, alpha: 1), "!", "Conflicted"),
    ("modified", NSColor(srgbRed: 0.85, green: 0.51, blue: 0.17, alpha: 1), "✱", "Modified"),
    ("deleted", NSColor(srgbRed: 0.70, green: 0.11, blue: 0.16, alpha: 1), "✕", "Deleted"),
    ("readonly", NSColor(srgbRed: 0.42, green: 0.45, blue: 0.49, alpha: 1), "R", "Assume valid"),
    ("locked", NSColor(srgbRed: 0.42, green: 0.45, blue: 0.49, alpha: 1), "■", "Skip worktree"),
    ("added", NSColor(srgbRed: 0.17, green: 0.48, blue: 0.85, alpha: 1), "+", "Added"),
    ("normal", NSColor(srgbRed: 0.18, green: 0.63, blue: 0.26, alpha: 1), "✓", "Normal"),
    ("ignored", NSColor(srgbRed: 0.55, green: 0.58, blue: 0.62, alpha: 1), "∅", "Ignored"),
    ("unversioned", NSColor(srgbRed: 0.55, green: 0.58, blue: 0.62, alpha: 1), "?", "Unversioned"),
]

private func badgeImage(color: NSColor, glyph: String) -> NSImage {
    NSImage(size: NSSize(width: 64, height: 64), flipped: false) { rect in
        let circle = NSBezierPath(ovalIn: rect.insetBy(dx: 3, dy: 3))
        color.setFill()
        circle.fill()
        NSColor.white.setStroke()
        circle.lineWidth = 5
        circle.stroke()
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.boldSystemFont(ofSize: 38),
            .foregroundColor: NSColor.white,
        ]
        let text = glyph as NSString
        let size = text.size(withAttributes: attrs)
        text.draw(at: NSPoint(x: rect.midX - size.width / 2, y: rect.midY - size.height / 2), withAttributes: attrs)
        return true
    }
}

/// The user's real home directory (the sandbox redirects `HOME` to the container).
private func realHome() -> URL {
    if let pw = getpwuid(getuid()), let dir = pw.pointee.pw_dir {
        return URL(fileURLWithPath: String(cString: dir))
    }
    return FileManager.default.homeDirectoryForCurrentUser
}

class FinderSync: FIFinderSync {
    private let queue = DispatchQueue(label: "koopagit.status")
    /// directory path -> (time, [file path: overlay id])
    private var statusCache: [String: (Date, [String: String])] = [:]
    /// Invoke arrays of the last menu, indexed by NSMenuItem.tag (Finder drops representedObject).
    private var menuActions: [[String]] = []

    private var hostApp: URL {
        // KoopaGit.app/Contents/PlugIns/KoopaGitFinderSync.appex
        Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    }

    private var helper: URL {
        hostApp.appendingPathComponent("Contents/MacOS/koopagit-shell")
    }

    /// The app icon on the "KoopaGit" submenu, as TortoiseGit's submenu shows its logo.
    private lazy var menuIcon: NSImage? = {
        let image = NSImage(contentsOf: hostApp.appendingPathComponent("Contents/Resources/AppIcon.icns"))
        image?.size = NSSize(width: 16, height: 16)
        return image
    }()

    override init() {
        super.init()
        let controller = FIFinderSyncController.default()
        controller.directoryURLs = Set(watchedRoots())
        for o in overlays {
            controller.setBadgeImage(badgeImage(color: o.color, glyph: o.glyph), label: o.label, forBadgeIdentifier: o.id)
        }
    }

    /// Finder Sync only observes configured folders: one path per line in
    /// `~/.config/koopagit/finder-roots`, defaulting to the home directory.
    private func watchedRoots() -> [URL] {
        let file = realHome().appendingPathComponent(".config/koopagit/finder-roots")
        guard let text = try? String(contentsOf: file, encoding: .utf8) else {
            return [realHome()]
        }
        let roots = text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .map { URL(fileURLWithPath: ($0 as NSString).expandingTildeInPath.replacingOccurrences(of: NSHomeDirectory(), with: realHome().path)) }
        return roots.isEmpty ? [realHome()] : roots
    }

    /// Runs the helper synchronously and returns stdout (nil on failure).
    private func runHelper(_ args: [String]) -> String? {
        let process = Process()
        process.executableURL = helper
        process.arguments = args
        var env = ProcessInfo.processInfo.environment
        env["HOME"] = realHome().path
        env["KOOPAGIT_SETTINGS"] = realHome().appendingPathComponent(".config/koopagit/settings.ini").path
        process.environment = env
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            return nil
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // MARK: - Badges

    override func requestBadgeIdentifier(for url: URL) {
        let dir = url.deletingLastPathComponent().path
        let path = url.path
        queue.async {
            let statuses = self.directoryStatus(dir)
            if let id = statuses[path], id != "none" {
                FIFinderSyncController.default().setBadgeIdentifier(id, for: url)
            }
        }
    }

    private func directoryStatus(_ dir: String) -> [String: String] {
        if let (time, cached) = statusCache[dir], Date().timeIntervalSince(time) < 2 {
            return cached
        }
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
        let paths = names.filter { $0 != ".git" }.map { (dir as NSString).appendingPathComponent($0) }
        var result: [String: String] = [:]
        if !paths.isEmpty, let out = runHelper(["status"] + paths) {
            for line in out.split(separator: "\n") {
                let parts = line.split(separator: "\t", maxSplits: 1).map(String.init)
                if parts.count == 2 { result[parts[1]] = parts[0] }
            }
        }
        statusCache[dir] = (Date(), result)
        return result
    }

    // MARK: - Context menu

    override func menu(for menuKind: FIMenuKind) -> NSMenu {
        let menu = NSMenu(title: "")
        menuActions = []
        let controller = FIFinderSyncController.default()
        var args = ["menu"]
        switch menuKind {
        case .contextualMenuForItems:
            let items = controller.selectedItemURLs() ?? []
            if items.isEmpty { return menu }
            args += ["--"] + items.map(\.path)
        case .contextualMenuForContainer:
            guard let target = controller.targetedURL() else { return menu }
            args += ["--background", target.path]
        default:
            return menu
        }
        if NSEvent.modifierFlags.contains(.shift) {
            args.insert("--extended", at: 1)
        }
        guard let out = runHelper(args),
              let data = out.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return menu }

        for node in json["top"] as? [[String: Any]] ?? [] {
            if let item = makeItem(node) { menu.addItem(item) }
        }
        if let sub = json["submenu"] as? [String: Any],
           let children = sub["children"] as? [[String: Any]], !children.isEmpty
        {
            let root = NSMenuItem(title: sub["label"] as? String ?? "KoopaGit", action: nil, keyEquivalent: "")
            root.image = menuIcon
            let submenu = NSMenu(title: root.title)
            for child in children {
                if let item = makeItem(child) { submenu.addItem(item) }
            }
            root.submenu = submenu
            menu.addItem(root)
        }
        return menu
    }

    private func makeItem(_ node: [String: Any]) -> NSMenuItem? {
        switch node["kind"] as? String {
        case "separator":
            return NSMenuItem.separator()
        case "submenu":
            let item = NSMenuItem(title: node["label"] as? String ?? "", action: nil, keyEquivalent: "")
            let submenu = NSMenu(title: item.title)
            for child in node["children"] as? [[String: Any]] ?? [] {
                if let c = makeItem(child) { submenu.addItem(c) }
            }
            item.submenu = submenu
            return item
        case "item":
            let item = NSMenuItem(title: node["label"] as? String ?? "", action: #selector(menuAction(_:)), keyEquivalent: "")
            item.toolTip = node["help"] as? String
            item.tag = menuActions.count
            menuActions.append(node["invoke"] as? [String] ?? [])
            return item
        default:
            return nil
        }
    }

    @IBAction func menuAction(_ sender: AnyObject?) {
        guard let item = sender as? NSMenuItem, menuActions.indices.contains(item.tag) else { return }
        let invoke = menuActions[item.tag]
        // Run through the (unsandboxed) host app so dialogs can write to repositories.
        // Launch arguments are dropped for sandboxed callers, so the invoke array
        // travels as a request file (in our container) that the host app opens.
        let request = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("koopagit-invoke")
        do {
            try JSONSerialization.data(withJSONObject: invoke).write(to: request)
        } catch {
            NSLog("koopagit: failed to write request: \(error)")
            return
        }
        let config = NSWorkspace.OpenConfiguration()
        config.createsNewApplicationInstance = true
        config.activates = false
        NSWorkspace.shared.open([request], withApplicationAt: hostApp, configuration: config) { _, error in
            if let error {
                NSLog("koopagit: failed to launch host app: \(error)")
            }
        }
    }
}
