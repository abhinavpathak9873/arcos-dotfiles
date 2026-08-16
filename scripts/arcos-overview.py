#!/usr/bin/env python3
"""ArcOS native workspace overview for Sway."""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk4LayerShell", "1.0")
from gi.repository import Gdk, Gio, GLib, Gtk, Gtk4LayerShell  # noqa: E402


@dataclass
class Window:
    con_id: int
    workspace: int
    app_id: str
    title: str
    rect: dict[str, int]
    focused: bool


def sway_json(message_type: str) -> object:
    return json.loads(subprocess.check_output(["swaymsg", "-r", "-t", message_type], text=True))


def run_sway(command: str) -> None:
    subprocess.run(["swaymsg", command], check=False, stdout=subprocess.DEVNULL)


def collect_windows(node: dict, workspace: int = -1) -> list[Window]:
    if node.get("type") == "workspace":
        try:
            workspace = int(str(node.get("name", "-1")).split(":", 1)[0])
        except ValueError:
            workspace = -1
    windows: list[Window] = []
    if node.get("pid") and workspace >= 0:
        props = node.get("window_properties") or {}
        app_id = node.get("app_id") or props.get("class") or props.get("instance") or "application"
        windows.append(
            Window(
                int(node["id"]),
                workspace,
                str(app_id),
                str(node.get("name") or app_id),
                node.get("rect") or {},
                bool(node.get("focused")),
            )
        )
    for child in (node.get("nodes") or []) + (node.get("floating_nodes") or []):
        windows.extend(collect_windows(child, workspace))
    return windows


class Overview(Gtk.Application):
    def __init__(self) -> None:
        super().__init__(application_id="dev.arcos.Overview", flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.windows = collect_windows(sway_json("get_tree"))
        self.workspaces = {int(item["num"]): item for item in sway_json("get_workspaces") if int(item["num"]) > 0}
        highest = max(self.workspaces, default=1)
        for number in range(1, max(4, highest + 1) + 1):
            self.workspaces.setdefault(number, {"num": number, "name": str(number), "focused": False, "rect": {}})
        self.icons = self._desktop_icons()

    @staticmethod
    def _desktop_icons() -> dict[str, Gio.Icon]:
        icons: dict[str, Gio.Icon] = {}
        for info in Gio.AppInfo.get_all():
            icon = info.get_icon()
            if not icon:
                continue
            keys = [info.get_id() or "", info.get_name() or "", info.get_executable() or ""]
            for key in keys:
                normalized = key.lower().removesuffix(".desktop")
                if normalized:
                    icons[normalized] = icon
                    icons[Path(normalized).name] = icon
        return icons

    def _icon(self, app_id: str, size: int = 28) -> Gtk.Image:
        key = app_id.lower().removesuffix(".desktop")
        icon = self.icons.get(key) or self.icons.get(Path(key).name)
        image = Gtk.Image.new_from_gicon(icon) if icon else Gtk.Image.new_from_icon_name("application-x-executable")
        image.set_pixel_size(size)
        return image

    def do_activate(self) -> None:
        window = Gtk.ApplicationWindow(application=self)
        window.set_name("arcos-overview")
        window.set_title("ArcOS Workspaces")
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
        css.load_from_path(str(Path.home() / ".config/arcos-desktop/overview.css"))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        key = Gtk.EventControllerKey()
        key.connect("key-pressed", self._key_pressed)
        window.add_controller(key)
        window.set_child(self._content())
        window.present()

    def _content(self) -> Gtk.Widget:
        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        root.add_css_class("overview-root")

        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        title_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        title = Gtk.Label(label="Workspaces", xalign=0)
        title.add_css_class("display-title")
        subtitle = Gtk.Label(label="Choose a workspace or jump straight to an open app", xalign=0)
        subtitle.add_css_class("subtitle")
        title_box.append(title)
        title_box.append(subtitle)
        header.append(title_box)
        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        header.append(spacer)
        hint = Gtk.Label(label="Esc  Close   ·   1–0  Switch workspace")
        hint.add_css_class("keyboard-hint")
        header.append(hint)
        root.append(header)

        cards = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
        cards.set_homogeneous(True)
        for number, workspace in sorted(self.workspaces.items()):
            cards.append(self._workspace_card(number, workspace))
        card_scroll = Gtk.ScrolledWindow()
        card_scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.NEVER)
        card_scroll.set_child(cards)
        card_scroll.set_propagate_natural_height(True)
        root.append(card_scroll)

        section = Gtk.Label(label="Open applications", xalign=0)
        section.add_css_class("section-title")
        root.append(section)

        app_list = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        for number in sorted(self.workspaces):
            workspace_windows = [item for item in self.windows if item.workspace == number]
            if not workspace_windows:
                continue
            heading = Gtk.Label(label=f"Workspace {number}", xalign=0)
            heading.add_css_class("workspace-heading")
            app_list.append(heading)
            for item in workspace_windows:
                app_list.append(self._app_row(item))
        if not self.windows:
            empty = Gtk.Label(label="No applications are open yet", xalign=0)
            empty.add_css_class("empty-state")
            app_list.append(empty)
        app_scroll = Gtk.ScrolledWindow()
        app_scroll.set_vexpand(True)
        app_scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        app_scroll.set_child(app_list)
        root.append(app_scroll)
        return root

    def _workspace_card(self, number: int, workspace: dict) -> Gtk.Button:
        button = Gtk.Button()
        button.add_css_class("workspace-card")
        if workspace.get("focused"):
            button.add_css_class("selected")
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        label = Gtk.Label(label=f"Workspace {number}", xalign=0)
        label.add_css_class("workspace-title")
        box.append(label)
        preview = Gtk.DrawingArea()
        preview.set_content_width(252)
        preview.set_content_height(142)
        preview.set_draw_func(self._draw_workspace, number)
        preview.add_css_class("workspace-preview")
        box.append(preview)
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=5)
        items = [item for item in self.windows if item.workspace == number]
        for item in items[:6]:
            row.append(self._icon(item.app_id, 20))
        count = Gtk.Label(label=(f"{len(items)} open" if items else "Empty"))
        count.add_css_class("workspace-count")
        row.append(count)
        box.append(row)
        button.set_child(box)
        button.connect("clicked", lambda _button: self._switch_workspace(number))
        return button

    def _draw_workspace(self, _area: Gtk.DrawingArea, cr, width: int, height: int, number: int) -> None:
        items = [item for item in self.windows if item.workspace == number]
        if not items:
            cr.set_source_rgba(1, 1, 1, 0.05)
            cr.rectangle(0, 0, width, height)
            cr.fill()
            return
        xs = [item.rect.get("x", 0) for item in items]
        ys = [item.rect.get("y", 0) for item in items]
        rights = [item.rect.get("x", 0) + max(item.rect.get("width", 1), 1) for item in items]
        bottoms = [item.rect.get("y", 0) + max(item.rect.get("height", 1), 1) for item in items]
        origin_x, origin_y = min(xs), min(ys)
        total_w, total_h = max(rights) - origin_x, max(bottoms) - origin_y
        scale = min((width - 12) / max(total_w, 1), (height - 12) / max(total_h, 1))
        for item in items:
            x = 6 + (item.rect.get("x", 0) - origin_x) * scale
            y = 6 + (item.rect.get("y", 0) - origin_y) * scale
            w = max(20, item.rect.get("width", 1) * scale - 4)
            h = max(16, item.rect.get("height", 1) * scale - 4)
            cr.set_source_rgba(0.79, 0.65, 0.97, 0.34 if not item.focused else 0.64)
            cr.rounded_rectangle(x, y, w, h, 7) if hasattr(cr, "rounded_rectangle") else cr.rectangle(x, y, w, h)
            cr.fill()
            cr.set_source_rgba(1, 1, 1, 0.86)
            cr.select_font_face("JetBrainsMono Nerd Font")
            cr.set_font_size(9)
            cr.move_to(x + 7, y + 14)
            cr.show_text(item.app_id[:20])

    def _app_row(self, item: Window) -> Gtk.Button:
        button = Gtk.Button()
        button.add_css_class("app-row")
        if item.focused:
            button.add_css_class("selected")
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        row.append(self._icon(item.app_id, 30))
        labels = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=1)
        title = Gtk.Label(label=item.title, xalign=0, ellipsize=3)
        title.add_css_class("app-title")
        app = Gtk.Label(label=item.app_id, xalign=0)
        app.add_css_class("app-subtitle")
        labels.append(title)
        labels.append(app)
        row.append(labels)
        button.set_child(row)
        button.connect("clicked", lambda _button: self._focus_window(item.con_id))
        return button

    def _switch_workspace(self, number: int) -> None:
        run_sway(f"workspace number {number}")
        self.quit()

    def _focus_window(self, con_id: int) -> None:
        run_sway(f"[con_id={con_id}] focus")
        self.quit()

    def _key_pressed(self, _controller, keyval: int, _keycode: int, _state: Gdk.ModifierType) -> bool:
        if keyval == Gdk.KEY_Escape:
            self.quit()
            return True
        name = Gdk.keyval_name(keyval) or ""
        if name.isdigit():
            self._switch_workspace(10 if name == "0" else int(name))
            return True
        return False


if __name__ == "__main__":
    raise SystemExit(Overview().run())
