#!/usr/bin/env python3
"""Centered native application launcher for the ArcOS Sway desktop."""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk4LayerShell", "1.0")
from gi.repository import Gdk, Gio, GLib, Gtk, Gtk4LayerShell  # noqa: E402


CATEGORY_RULES: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("Development", ("development", "ide", "texteditor")),
    ("Internet", ("network", "webbrowser", "email", "instantmessaging")),
    ("Office", ("office", "wordprocessor", "spreadsheet", "presentation")),
    ("Media", ("audio", "video", "graphics", "photography")),
    ("System", ("system", "settings", "utility", "filesystem", "monitor")),
)


@dataclass
class ApplicationEntry:
    info: Gio.AppInfo
    name: str
    description: str
    category: str
    search_text: str


def category_for(info: Gio.AppInfo) -> str:
    getter: Callable[[], str | None] | None = getattr(info, "get_categories", None)
    raw = (getter() if getter else "") or ""
    normalized = raw.casefold()
    for category, terms in CATEGORY_RULES:
        if any(term in normalized for term in terms):
            return category
    return "System"


def installed_apps() -> list[ApplicationEntry]:
    entries: list[ApplicationEntry] = []
    seen: set[str] = set()
    for info in Gio.AppInfo.get_all():
        if not info.should_show():
            continue
        app_id = info.get_id() or info.get_executable() or info.get_name()
        if not app_id or app_id in seen:
            continue
        seen.add(app_id)
        name = info.get_display_name() or info.get_name() or app_id
        description = info.get_description() or ""
        category = category_for(info)
        entries.append(
            ApplicationEntry(
                info=info,
                name=name,
                description=description,
                category=category,
                search_text=" ".join((name, description, app_id, category)).casefold(),
            )
        )
    return sorted(entries, key=lambda item: item.name.casefold())


class Applications(Gtk.Application):
    def __init__(self) -> None:
        super().__init__(application_id="dev.arcos.Applications", flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.entries = installed_apps()
        self.tiles: list[tuple[ApplicationEntry, Gtk.Widget]] = []
        self.category = "All"
        self.category_buttons: dict[str, Gtk.Button] = {}
        self.first_match: ApplicationEntry | None = None
        self.search = Gtk.SearchEntry(placeholder_text="Search applications")
        self.search.add_css_class("apps-search")
        self.flow = Gtk.FlowBox()
        self.ready = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / "arcos-apps.ready"
        self.ready.unlink(missing_ok=True)

    def do_activate(self) -> None:
        window = Gtk.ApplicationWindow(application=self)
        window.set_name("arcos-apps")
        window.set_title("ArcOS Applications")
        window.set_default_size(900, 600)
        window.set_resizable(False)
        Gtk4LayerShell.init_for_window(window)
        Gtk4LayerShell.set_layer(window, Gtk4LayerShell.Layer.OVERLAY)
        Gtk4LayerShell.set_keyboard_mode(window, Gtk4LayerShell.KeyboardMode.EXCLUSIVE)

        css = Gtk.CssProvider()
        css.load_from_path(str(Path.home() / ".config/arcos-desktop/apps.css"))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        keys = Gtk.EventControllerKey()
        keys.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        keys.connect("key-pressed", self._key_pressed)
        window.add_controller(keys)
        window.set_child(self._content())
        window.present()
        self.search.grab_focus()
        GLib.idle_add(self._mark_ready)

    def _mark_ready(self) -> bool:
        self.ready.touch()
        return GLib.SOURCE_REMOVE

    def _content(self) -> Gtk.Widget:
        surface = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        surface.add_css_class("apps-surface")
        for setter in (surface.set_margin_top, surface.set_margin_end, surface.set_margin_bottom, surface.set_margin_start):
            setter(10)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        panel.add_css_class("apps-panel")

        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        title = Gtk.Label(label="Applications", xalign=0)
        title.add_css_class("display-title")
        subtitle = Gtk.Label(label="Everything installed, one search away", xalign=0)
        subtitle.add_css_class("subtitle")
        titles.append(title)
        titles.append(subtitle)
        header.append(titles)
        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        header.append(spacer)
        hint = Gtk.Label(label="Esc  Close   ·   Enter  Open")
        hint.add_css_class("keyboard-hint")
        header.append(hint)
        panel.append(header)

        self.search.connect("search-changed", self._filter)
        self.search.connect("activate", lambda _entry: self._launch(self.first_match))
        panel.append(self.search)

        categories = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=7)
        categories.add_css_class("category-strip")
        for name in ("All", "Development", "Internet", "Office", "Media", "System"):
            button = Gtk.Button(label=name)
            button.add_css_class("category-chip")
            button.connect("clicked", lambda _button, category=name: self._select_category(category))
            self.category_buttons[name] = button
            categories.append(button)
        self.category_buttons["All"].add_css_class("selected")
        panel.append(categories)

        self.flow.set_selection_mode(Gtk.SelectionMode.NONE)
        self.flow.set_homogeneous(True)
        self.flow.set_min_children_per_line(5)
        self.flow.set_max_children_per_line(5)
        self.flow.set_column_spacing(8)
        self.flow.set_row_spacing(8)
        for entry in self.entries:
            tile = self._tile(entry)
            self.tiles.append((entry, tile))
            self.flow.append(tile)

        scroll = Gtk.ScrolledWindow()
        scroll.set_vexpand(True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_child(self.flow)
        panel.append(scroll)
        surface.append(panel)
        self._filter()
        return surface

    def _tile(self, entry: ApplicationEntry) -> Gtk.Button:
        button = Gtk.Button()
        button.add_css_class("app-tile")
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        icon = entry.info.get_icon()
        image = Gtk.Image.new_from_gicon(icon) if icon else Gtk.Image.new_from_icon_name("application-x-executable")
        image.set_pixel_size(46)
        content.append(image)
        label = Gtk.Label(label=entry.name, max_width_chars=17, ellipsize=3)
        label.add_css_class("app-name")
        content.append(label)
        button.set_child(content)
        button.connect("clicked", lambda _button: self._launch(entry))
        return button

    def _select_category(self, category: str) -> None:
        self.category = category
        for name, button in self.category_buttons.items():
            if name == category:
                button.add_css_class("selected")
            else:
                button.remove_css_class("selected")
        self._filter()

    def _filter(self, *_args) -> None:
        query = self.search.get_text().strip().casefold()
        self.first_match = None
        for entry, tile in self.tiles:
            visible = (self.category == "All" or entry.category == self.category) and (
                not query or query in entry.search_text
            )
            tile.set_visible(visible)
            if visible and self.first_match is None:
                self.first_match = entry

    def _launch(self, entry: ApplicationEntry | None) -> None:
        if entry is None:
            return
        try:
            entry.info.launch([], None)
        except Exception as error:  # GLib errors need to remain visible to the caller's log.
            print(f"Unable to launch {entry.name}: {error}")
            return
        self.quit()

    def _key_pressed(self, _controller, keyval: int, _keycode: int, _state: Gdk.ModifierType) -> bool:
        if keyval == Gdk.KEY_Escape:
            self.quit()
            return True
        if keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter):
            self._launch(self.first_match)
            return True
        return False


if __name__ == "__main__":
    raise SystemExit(Applications().run())
