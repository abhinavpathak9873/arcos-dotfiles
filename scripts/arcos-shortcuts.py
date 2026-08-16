#!/usr/bin/env python3
"""Searchable, context-aware ArcOS keyboard shortcut reference."""

from __future__ import annotations

import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk4LayerShell", "1.0")
from gi.repository import Gdk, Gio, Gtk, Gtk4LayerShell  # noqa: E402


@dataclass(frozen=True)
class Shortcut:
    keys: str
    action: str
    group: str
    apps: tuple[str, ...] = ()


SHORTCUTS = (
    Shortcut("Meta + Space", "Search running windows, apps, files, settings, calculations, or the web", "Find & launch"),
    Shortcut("Meta + A", "Open the full application grid", "Find & launch"),
    Shortcut("Meta + W", "Open the visual workspace overview", "Find & launch"),
    Shortcut("Meta + V", "Search clipboard history, including copied images", "Find & launch"),
    Shortcut("Meta + Shift + K", "Open this context-aware shortcut guide", "Find & launch"),
    Shortcut("Meta + Enter", "Open a terminal", "Applications"),
    Shortcut("Meta + E", "Open Files", "Applications"),
    Shortcut("Meta + B", "Open Google Chrome", "Applications"),
    Shortcut("Meta + comma", "Open System Settings", "Applications"),
    Shortcut("Print", "Open the screenshot and recording menu", "Applications"),
    Shortcut("Meta + 1…0", "Switch to workspace 1…10", "Workspaces"),
    Shortcut("Meta + Shift + 1…0", "Move the focused window to workspace 1…10", "Workspaces"),
    Shortcut("Meta + Ctrl + ← / →", "Switch to the previous or next workspace", "Workspaces"),
    Shortcut("Meta + Tab", "Focus the next open window", "Windows"),
    Shortcut("Alt + Tab", "Focus the next open window", "Windows"),
    Shortcut("Meta + arrows", "Move focus between tiled windows", "Windows"),
    Shortcut("Meta + Shift + arrows", "Move the focused tiled window", "Windows"),
    Shortcut("Meta + F", "Toggle true fullscreen", "Windows"),
    Shortcut("Meta + Shift + Space", "Toggle floating mode", "Windows"),
    Shortcut("Meta + R", "Enter resize mode; arrows resize; Enter exits", "Windows"),
    Shortcut("Meta + Q", "Close the focused window", "Windows"),
    Shortcut("Meta + Ctrl + L", "Lock the desktop", "System"),
    Shortcut("Meta + N", "Open notifications and quick controls", "System"),
    Shortcut("Meta + Escape", "Open the lock, suspend, restart, and power menu", "System"),
    Shortcut("Hold Caps Lock", "Dictate into the focused text field; release to transcribe", "Voice"),
    Shortcut("Ctrl + Shift + C", "Copy from Kitty", "Kitty", ("kitty",)),
    Shortcut("Ctrl + Shift + V", "Paste into Kitty", "Kitty", ("kitty",)),
    Shortcut("Ctrl + Shift + T", "Open a new Kitty tab", "Kitty", ("kitty",)),
    Shortcut("Ctrl + Shift + Enter", "Open a new Kitty window", "Kitty", ("kitty",)),
    Shortcut("Ctrl + L", "Focus the location bar", "Google Chrome", ("chrome", "google-chrome")),
    Shortcut("Ctrl + T", "Open a new tab", "Google Chrome", ("chrome", "google-chrome")),
    Shortcut("Ctrl + Shift + T", "Reopen the last closed tab", "Google Chrome", ("chrome", "google-chrome")),
    Shortcut("Ctrl + L", "Edit the current folder location", "Files", ("nautilus", "org.gnome.nautilus")),
    Shortcut("Ctrl + Shift + N", "Create a folder", "Files", ("nautilus", "org.gnome.nautilus")),
    Shortcut("Ctrl + .", "Show hidden files", "Files", ("nautilus", "org.gnome.nautilus")),
    Shortcut("Ctrl + Shift + P", "Open the command palette", "VS Code", ("code", "vscode")),
    Shortcut("Ctrl + P", "Quick-open a file", "VS Code", ("code", "vscode")),
)


def focused_app() -> str:
    try:
        tree = json.loads(subprocess.check_output(["swaymsg", "-r", "-t", "get_tree"], text=True))
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError):
        return "desktop"

    def visit(node: dict) -> str | None:
        if node.get("focused"):
            props = node.get("window_properties") or {}
            return str(node.get("app_id") or props.get("class") or props.get("instance") or "desktop")
        for child in (node.get("nodes") or []) + (node.get("floating_nodes") or []):
            if result := visit(child):
                return result
        return None

    return (visit(tree) or "desktop").lower()


class ShortcutGuide(Gtk.Application):
    def __init__(self) -> None:
        super().__init__(application_id="dev.arcos.Shortcuts", flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.context = focused_app()
        self.rows = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)

    def do_activate(self) -> None:
        window = Gtk.ApplicationWindow(application=self)
        window.set_name("arcos-shortcuts")
        window.set_title("ArcOS Shortcuts")
        Gtk4LayerShell.init_for_window(window)
        Gtk4LayerShell.set_layer(window, Gtk4LayerShell.Layer.OVERLAY)
        Gtk4LayerShell.set_keyboard_mode(window, Gtk4LayerShell.KeyboardMode.EXCLUSIVE)
        for edge in (Gtk4LayerShell.Edge.TOP, Gtk4LayerShell.Edge.RIGHT, Gtk4LayerShell.Edge.BOTTOM, Gtk4LayerShell.Edge.LEFT):
            Gtk4LayerShell.set_anchor(window, edge, True)

        css = Gtk.CssProvider()
        css.load_from_path(str(Path.home() / ".config/arcos-desktop/shortcuts.css"))
        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

        keys = Gtk.EventControllerKey()
        keys.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        keys.connect("key-pressed", self._key_pressed)
        window.add_controller(keys)
        window.set_child(self._content())
        window.connect("map", self._mapped)
        window.present()

    @staticmethod
    def _mapped(_window: Gtk.Window) -> None:
        runtime = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
        (runtime / "arcos-shortcuts.ready").touch(mode=0o600)

    def _content(self) -> Gtk.Widget:
        backdrop = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        backdrop.add_css_class("shortcut-backdrop")
        backdrop.set_valign(Gtk.Align.CENTER)
        backdrop.set_halign(Gtk.Align.FILL)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18)
        panel.add_css_class("shortcut-panel")
        panel.set_halign(Gtk.Align.CENTER)
        panel.set_size_request(850, 680)

        heading = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
        titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        title = Gtk.Label(label="Keyboard shortcuts", xalign=0)
        title.add_css_class("shortcut-title")
        context = Gtk.Label(label=f"Showing global shortcuts and context for {self.context}", xalign=0)
        context.add_css_class("shortcut-subtitle")
        titles.append(title)
        titles.append(context)
        heading.append(titles)
        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        heading.append(spacer)
        escape = Gtk.Label(label="Esc  Close")
        escape.add_css_class("shortcut-hint")
        heading.append(escape)
        panel.append(heading)

        search = Gtk.SearchEntry(placeholder_text="Search keys or actions")
        search.add_css_class("shortcut-search")
        search.connect("search-changed", self._filter)
        panel.append(search)

        scroll = Gtk.ScrolledWindow()
        scroll.set_vexpand(True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_child(self.rows)
        panel.append(scroll)
        backdrop.append(panel)
        self._render("")
        search.grab_focus()
        return backdrop

    def _eligible(self, item: Shortcut) -> bool:
        return not item.apps or any(name in self.context for name in item.apps)

    def _render(self, query: str) -> None:
        while child := self.rows.get_first_child():
            self.rows.remove(child)
        query = query.casefold().strip()
        groups: dict[str, list[Shortcut]] = {}
        for item in SHORTCUTS:
            searchable = f"{item.keys} {item.action} {item.group}".casefold()
            if self._eligible(item) and (not query or query in searchable):
                groups.setdefault(item.group, []).append(item)
        for group, items in groups.items():
            label = Gtk.Label(label=group, xalign=0)
            label.add_css_class("shortcut-group")
            self.rows.append(label)
            for item in items:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=18)
                row.add_css_class("shortcut-row")
                action = Gtk.Label(label=item.action, xalign=0, wrap=True)
                action.set_hexpand(True)
                action.add_css_class("shortcut-action")
                key = Gtk.Label(label=item.keys)
                key.add_css_class("shortcut-key")
                row.append(action)
                row.append(key)
                self.rows.append(row)
        if not groups:
            empty = Gtk.Label(label="No matching shortcuts", xalign=0)
            empty.add_css_class("shortcut-empty")
            self.rows.append(empty)

    def _filter(self, entry: Gtk.SearchEntry) -> None:
        self._render(entry.get_text())

    def _key_pressed(self, _controller, keyval: int, _keycode: int, _state: Gdk.ModifierType) -> bool:
        if keyval == Gdk.KEY_Escape:
            self.quit()
            return True
        return False


if __name__ == "__main__":
    raise SystemExit(ShortcutGuide().run())
