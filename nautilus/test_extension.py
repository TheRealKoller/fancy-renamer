"""Tests der Nautilus-Erweiterung ohne Nautilus (``python3 nautilus/test_extension.py``).

Die PyGObject-Module werden durch Attrappen ersetzt, damit der Test auf jedem
Rechner ohne Nautilus läuft.
"""

import importlib.util
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path

EXTENSION = Path(__file__).with_name("fancy-renamer.py")


class FakeMenuItem:
    def __init__(self, name, label, tip=None):
        self.name = name
        self.label = label
        self.tip = tip
        self._handlers = {}

    def connect(self, signal, callback, *args):
        self._handlers[signal] = (callback, args)

    def click(self):
        callback, args = self._handlers["activate"]
        callback(self, *args)


class FakeLocation:
    def __init__(self, path):
        self._path = path

    def get_path(self):
        return self._path


class FakeFileInfo:
    def __init__(self, path):
        self._location = FakeLocation(path) if path is not None else None

    def get_location(self):
        return self._location


def install_stubs():
    gi = types.ModuleType("gi")
    repository = types.ModuleType("gi.repository")
    gobject = types.SimpleNamespace(GObject=type("GObject", (), {}))
    nautilus = types.SimpleNamespace(
        MenuProvider=type("MenuProvider", (), {}),
        MenuItem=FakeMenuItem,
    )
    repository.GObject = gobject
    repository.Nautilus = nautilus
    gi.repository = repository
    sys.modules["gi"] = gi
    sys.modules["gi.repository"] = repository


def load_extension():
    install_stubs()
    spec = importlib.util.spec_from_file_location("fancy_renamer_nautilus", EXTENSION)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ExtensionTest(unittest.TestCase):
    def setUp(self):
        self.extension = load_extension()
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(self._restore_env, dict(os.environ))

    def _restore_env(self, previous):
        os.environ.clear()
        os.environ.update(previous)

    def fake_binary(self, directory, name="fancy-renamer"):
        path = Path(directory) / name
        path.write_text("#!/bin/sh\n")
        path.chmod(0o755)
        return str(path)

    def test_binary_from_override(self):
        binary = self.fake_binary(self.temp.name)
        os.environ["FANCY_RENAMER_BIN"] = binary
        self.assertEqual(self.extension.find_binary(), binary)

    def test_binary_from_xdg_bin_home(self):
        os.environ.pop("FANCY_RENAMER_BIN", None)
        binary = self.fake_binary(self.temp.name)
        os.environ["XDG_BIN_HOME"] = self.temp.name
        self.assertEqual(self.extension.find_binary(), binary)

    def test_missing_binary_yields_no_menu_item(self):
        os.environ.pop("FANCY_RENAMER_BIN", None)
        os.environ["XDG_BIN_HOME"] = self.temp.name
        os.environ["HOME"] = self.temp.name
        os.environ["PATH"] = self.temp.name
        self.assertIsNone(self.extension.find_binary())
        provider = self.extension.FancyRenamerExtension()
        self.assertEqual(provider.get_file_items([FakeFileInfo("/tmp/a.jpg")]), [])

    def test_menu_item_passes_selected_paths(self):
        binary = self.fake_binary(self.temp.name)
        self.extension.find_binary = lambda: binary
        started = []
        self.extension.launch = lambda bin_path, paths: started.append((bin_path, paths))

        provider = self.extension.FancyRenamerExtension()
        items = provider.get_file_items([FakeFileInfo("/tmp/a.jpg"), FakeFileInfo("/tmp/b.jpg")])
        self.assertEqual(len(items), 1)
        self.assertIn("fancy-renamer", items[0].label)
        items[0].click()
        self.assertEqual(started, [(binary, ["/tmp/a.jpg", "/tmp/b.jpg"])])

    def test_selection_without_local_path_is_ignored(self):
        self.extension.find_binary = lambda: "/bin/true"
        provider = self.extension.FancyRenamerExtension()
        self.assertEqual(provider.get_file_items([FakeFileInfo(None)]), [])


if __name__ == "__main__":
    unittest.main()
