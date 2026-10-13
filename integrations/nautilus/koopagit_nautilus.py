"""koopagit Nautilus extension (nautilus-python 4.x).

Thin by design: every decision is made by the `koopagit-shell` helper.
- Context menus come from `koopagit-shell menu ... ` (JSON).
- Emblems come from `koopagit-shell status <paths...>`.
- Clicking an item runs `koopagit-shell invoke <invoke...>`, which starts the
  launcher detached, so a crash or hang never affects Nautilus.
"""

import json
import os
import shutil
import subprocess
import time
from urllib.parse import unquote, urlparse

from gi.repository import GObject, Nautilus

HELPER = os.environ.get("KOOPAGIT_SHELL") or shutil.which("koopagit-shell") or "koopagit-shell"
TIMEOUT = 5  # seconds; never block the file manager for long
STATUS_TTL = 2.0  # seconds a directory's status listing is reused

# Overlay ids printed by `koopagit-shell status` -> installed emblem icon names.
EMBLEMS = {
    "conflict": "emblem-koopagit-conflict",
    "modified": "emblem-koopagit-modified",
    "deleted": "emblem-koopagit-deleted",
    "readonly": "emblem-koopagit-readonly",
    "locked": "emblem-koopagit-locked",
    "added": "emblem-koopagit-added",
    "normal": "emblem-koopagit-normal",
    "ignored": "emblem-koopagit-ignored",
    "unversioned": "emblem-koopagit-unversioned",
}


def _local_path(file_info):
    uri = file_info.get_uri()
    if not uri.startswith("file://"):
        return None
    return unquote(urlparse(uri).path)


def _run(args):
    try:
        out = subprocess.run(
            [HELPER] + args,
            capture_output=True,
            text=True,
            timeout=TIMEOUT,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    return out.stdout


def _invoke(args):
    try:
        subprocess.Popen(
            [HELPER, "invoke"] + list(args),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
    except OSError:
        pass


class KoopaGitExtension(GObject.GObject, Nautilus.MenuProvider, Nautilus.InfoProvider):
    def __init__(self):
        super().__init__()
        # directory -> (timestamp, {path: overlay id})
        self._status_cache = {}

    # ---- emblems -----------------------------------------------------------

    def _dir_status(self, directory):
        cached = self._status_cache.get(directory)
        now = time.monotonic()
        if cached and now - cached[0] < STATUS_TTL:
            return cached[1]
        try:
            names = os.listdir(directory)
        except OSError:
            names = []
        paths = [os.path.join(directory, n) for n in names if n != ".git"]
        result = {}
        if paths:
            out = _run(["status"] + paths) or ""
            for line in out.splitlines():
                overlay, _, path = line.partition("\t")
                result[path] = overlay
        self._status_cache[directory] = (now, result)
        return result

    def update_file_info(self, file):
        path = _local_path(file)
        if not path:
            return
        overlay = self._dir_status(os.path.dirname(path)).get(path)
        emblem = EMBLEMS.get(overlay or "")
        if emblem:
            file.add_emblem(emblem)

    # ---- context menu ------------------------------------------------------

    def _build_items(self, nodes, prefix):
        items = []
        for i, node in enumerate(nodes):
            kind = node.get("kind")
            if kind == "separator":
                # Nautilus menus have no separators; group boundaries are implicit.
                continue
            name = "KoopaGit::%s_%d" % (prefix, i)
            item = Nautilus.MenuItem(name=name, label=node.get("label", ""), tip=node.get("help", ""))
            if kind == "submenu":
                sub = Nautilus.Menu()
                for child in self._build_items(node.get("children", []), "%s_%d" % (prefix, i)):
                    sub.append_item(child)
                item.set_submenu(sub)
            else:
                item.connect("activate", lambda _item, args=node.get("invoke", []): _invoke(args))
            items.append(item)
        return items

    def _menu(self, args):
        out = _run(["menu"] + args)
        if not out:
            return []
        try:
            menu = json.loads(out)
        except ValueError:
            return []
        items = self._build_items(menu.get("top", []), "top")
        sub = menu.get("submenu", {})
        children = self._build_items(sub.get("children", []), "sub")
        if children:
            root = Nautilus.MenuItem(name="KoopaGit::submenu", label=sub.get("label", "KoopaGit"))
            submenu = Nautilus.Menu()
            for child in children:
                submenu.append_item(child)
            root.set_submenu(submenu)
            items.append(root)
        return items

    def get_file_items(self, files):
        paths = [p for p in (_local_path(f) for f in files) if p]
        if not paths or len(paths) != len(files):
            return []
        return self._menu(["--"] + paths)

    def get_background_items(self, folder):
        path = _local_path(folder)
        if not path:
            return []
        return self._menu(["--background", path])
