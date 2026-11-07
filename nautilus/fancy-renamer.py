"""Nautilus-Kontextmenü: markierte Dateien an fancy-renamer übergeben.

Installation: ``./install.sh --nautilus`` kopiert diese Datei nach
``$XDG_DATA_HOME/nautilus-python/extensions/``. Danach Nautilus neu starten
(``nautilus -q``). Voraussetzung ist das Paket ``nautilus-python``.
"""

import os
import shutil
import subprocess
import sys

from gi.repository import GObject, Nautilus

APP_NAME = "fancy-renamer"
BINARY_ENV = "FANCY_RENAMER_BIN"


def binary_candidates():
    """Mögliche Pfade der Binärdatei, in Prüfreihenfolge."""
    override = os.environ.get(BINARY_ENV)
    if override:
        yield override
    bin_home = os.environ.get("XDG_BIN_HOME")
    if bin_home:
        yield os.path.join(bin_home, APP_NAME)
    yield os.path.join(os.path.expanduser("~/.local/bin"), APP_NAME)
    found = shutil.which(APP_NAME)
    if found:
        yield found


def find_binary():
    """Erste ausführbare Binärdatei oder ``None``."""
    for candidate in binary_candidates():
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def selected_paths(files):
    """Lokale Pfade der markierten Einträge; ``None`` bei reinen URI-Objekten."""
    paths = []
    for file_info in files:
        location = file_info.get_location()
        path = location.get_path() if location is not None else None
        if not path:
            return None
        paths.append(path)
    return paths


def launch(binary, paths):
    """Startet die App ohne Terminallog und ohne Wait."""
    return subprocess.Popen(
        [binary, *paths],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


class FancyRenamerExtension(GObject.GObject, Nautilus.MenuProvider):
    """Eintrag „Mit fancy-renamer umbenennen…“ für die Auswahl."""

    def get_file_items(self, *args):
        files = args[-1] if args else []
        if not files:
            return []
        paths = selected_paths(files)
        if not paths:
            return []
        binary = find_binary()
        if binary is None:
            return []
        item = Nautilus.MenuItem(
            name="FancyRenamer::rename",
            label="Mit fancy-renamer umbenennen…",
            tip="Die markierten Dateien in fancy-renamer öffnen",
        )
        item.connect("activate", self._activate, binary, paths)
        return [item]

    def _activate(self, _menu_item, binary, paths):
        try:
            launch(binary, paths)
        except OSError as error:
            print(f"fancy-renamer konnte nicht gestartet werden: {error}", file=sys.stderr)
