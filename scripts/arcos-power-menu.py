#!/usr/bin/env python3
"""ArcOS native session and power surface for Sway."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk4LayerShell", "1.0")
from gi.repository import Gdk, Gio, Gtk, Gtk4LayerShell  # noqa: E402


ACTIONS = (
    ("Lock", "Keep everything running", "system-lock-screen-symbolic", "arcos-lock"),
    ("Suspend", "Pause this computer", "media-playback-pause-symbolic", "systemctl suspend"),
    ("Log out", "End the Sway session", "system-log-out-symbolic", "swaymsg exit"),
    ("Restart", "Reboot ArcOS", "view-refresh-symbolic", "systemctl reboot"),
    ("Shut down", "Power off safely", "system-shutdown-symbolic", "systemctl poweroff"),
)


class PowerMenu(Gtk.Application):
    def __init__(self) -> None:
        super().__init__(application_id="dev.arcos.PowerMenu", flags=Gio.ApplicationFlags.NON_UNIQUE)

    def do_activate(self) -> None:
        window = Gtk.ApplicationWindow(application=self)
        window.set_name("arcos-power-menu")
        window.set_title("ArcOS Session")
        Gtk4LayerShell.init_for_window(window)
        Gtk4LayerShell.set_layer(window, Gtk4LayerShell.Layer.OVERLAY)
        Gtk4LayerShell.set_keyboard_mode(window, Gtk4LayerShell.KeyboardMode.EXCLUSIVE)
        for edge in (
            Gtk4LayerShell.Edge.TOP,
            Gtk4LayerShell.Edge.RIGHT,
            Gtk4LayerShell.Edge.BOTTOM,
            Gtk4LayerShell.Edge.LEFT,
        ):
            Gtk4LayerShell.set_anchor(window, edge, True)

        css = Gtk.CssProvider()
        css.load_from_path(str(Path.home() / ".config/arcos-desktop/nwg-bar.css"))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        key = Gtk.EventControllerKey()
        key.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        key.connect("key-pressed", self._key_pressed)
        window.add_controller(key)
        window.set_child(self._content())
        window.connect("map", self._mapped)
        window.present()

    @staticmethod
    def _mapped(_window: Gtk.Window) -> None:
        runtime = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
        (runtime / "arcos-power-menu.ready").touch(mode=0o600)

    def _content(self) -> Gtk.Widget:
        shell = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        shell.add_css_class("power-root")
        before = Gtk.Box()
        before.set_vexpand(True)
        shell.append(before)

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        root.add_css_class("power-panel")
        root.set_halign(Gtk.Align.CENTER)

        heading = Gtk.Label(label="What would you like to do?")
        heading.add_css_class("power-title")
        root.append(heading)
        hint = Gtk.Label(label="Choose an action  ·  Esc to return")
        hint.add_css_class("power-hint")
        root.append(hint)

        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        actions.add_css_class("power-actions")
        for label, description, icon_name, command in ACTIONS:
            button = Gtk.Button()
            button.add_css_class("power-action")
            content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(36)
            icon.add_css_class("power-icon")
            title = Gtk.Label(label=label)
            title.add_css_class("power-label")
            detail = Gtk.Label(label=description, wrap=True, justify=Gtk.Justification.CENTER)
            detail.add_css_class("power-description")
            content.append(icon)
            content.append(title)
            content.append(detail)
            button.set_child(content)
            button.connect("clicked", self._run, command)
            actions.append(button)
        root.append(actions)
        shell.append(root)
        after = Gtk.Box()
        after.set_vexpand(True)
        shell.append(after)
        return shell

    def _run(self, _button: Gtk.Button, command: str) -> None:
        self.quit()
        subprocess.Popen(command.split(), start_new_session=True)

    def _key_pressed(self, _controller, keyval: int, _keycode: int, _state: Gdk.ModifierType) -> bool:
        if keyval == Gdk.KEY_Escape:
            self.quit()
            return True
        return False


if __name__ == "__main__":
    raise SystemExit(PowerMenu().run())
