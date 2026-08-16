#!/usr/bin/env python3
"""Generate and apply a restrained ArcOS desktop palette."""

from __future__ import annotations

import colorsys
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path


DEFAULT_ACCENT = "#cba6f7"
HEX = re.compile(r"^#?([0-9a-fA-F]{6})$")
RGB = re.compile(r"^rgb\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$")
HISTOGRAM = re.compile(r"^\s*(\d+):.*#([0-9A-Fa-f]{6})(?:[0-9A-Fa-f]{2})?\b")


def fail(message: str) -> None:
    print(f"arcos-theme: {message}", file=sys.stderr)
    raise SystemExit(2)


def parse_color(value: str) -> str:
    if match := HEX.match(value.strip()):
        return f"#{match.group(1).lower()}"
    if match := RGB.match(value.strip()):
        channels = [min(255, int(match.group(index))) for index in range(1, 4)]
        return "#" + "".join(f"{channel:02x}" for channel in channels)
    fail("color must be #RRGGBB or rgb(R,G,B)")


def rgb(hex_color: str) -> tuple[float, float, float]:
    value = hex_color.removeprefix("#")
    return tuple(int(value[index : index + 2], 16) / 255 for index in (0, 2, 4))


def hex_color(red: float, green: float, blue: float) -> str:
    channels = (red, green, blue)
    return "#" + "".join(f"{round(max(0, min(1, channel)) * 255):02x}" for channel in channels)


def hsl_color(hue: float, saturation: float, lightness: float) -> str:
    return hex_color(*colorsys.hls_to_rgb(hue, lightness, saturation))


def choose_accent(wallpaper: Path) -> str:
    magick = shutil.which("magick")
    if magick is None:
        return DEFAULT_ACCENT
    process = subprocess.run(
        [
            magick,
            str(wallpaper),
            "-auto-orient",
            "-resize",
            "160x160^",
            "-gravity",
            "center",
            "-extent",
            "160x160",
            "-colors",
            "20",
            "-format",
            "%c",
            "histogram:info:-",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    candidates: list[tuple[float, str]] = []
    for line in process.stdout.splitlines():
        match = HISTOGRAM.match(line)
        if not match:
            continue
        count = int(match.group(1))
        candidate = f"#{match.group(2).lower()}"
        hue, lightness, saturation = colorsys.rgb_to_hls(*rgb(candidate))
        if lightness < 0.12 or lightness > 0.9 or saturation < 0.12:
            continue
        readable_lightness = 1 - min(abs(lightness - 0.55) / 0.55, 1)
        score = saturation * 0.62 + readable_lightness * 0.25 + min(count / 7000, 1) * 0.13
        candidates.append((score, candidate))
    return max(candidates, default=(0, DEFAULT_ACCENT))[1]


def palette(accent: str) -> dict[str, str]:
    hue, _lightness, saturation = colorsys.rgb_to_hls(*rgb(accent))
    # Wallpaper colors are intentionally normalized into a calm middle range.
    # The wallpaper supplies the hue; the desktop controls the intensity so a
    # neon or very bright image never turns the shell into a neon theme.
    saturation = max(0.42, min(saturation, 0.66))
    return {
        "background": hsl_color(hue, 0.16, 0.075),
        "background_alt": hsl_color(hue, 0.18, 0.115),
        "panel": hsl_color(hue, 0.2, 0.145),
        "border": hsl_color(hue, 0.2, 0.27),
        "foreground": hsl_color(hue, 0.13, 0.91),
        "muted": hsl_color(hue, 0.16, 0.67),
        "accent": hsl_color(hue, saturation, 0.62),
        "accent_soft": hsl_color(hue, saturation * 0.72, 0.52),
        "accent_text": hsl_color(hue, 0.24, 0.12),
        "warning": "#f9e2af",
        "critical": "#f38ba8",
    }


def render(template: Path, colors: dict[str, str]) -> str:
    result = template.read_text(encoding="utf-8")
    for name, value in colors.items():
        result = result.replace("{{" + name + "}}", value)
    return result


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".new")
    temporary.write_text(content, encoding="utf-8")
    temporary.replace(path)


def run_quiet(command: list[str]) -> None:
    if shutil.which(command[0]):
        subprocess.run(command, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def apply_theme(wallpaper: Path, accent: str, source: str, *, reload_desktop: bool = True) -> None:
    if not wallpaper.is_file():
        fail(f"wallpaper does not exist: {wallpaper}")

    template_dir = Path(os.environ.get("ARCOS_THEME_TEMPLATE_DIR", "/etc/arcos-desktop/templates"))
    config_home = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    theme_dir = config_home / "arcos-desktop"
    theme_dir.mkdir(parents=True, exist_ok=True)
    colors = palette(accent)
    colors["wallpaper"] = str(wallpaper)

    for template_name, target_name in (
        ("waybar.css", "waybar.css"),
        ("rofi.rasi", "rofi.rasi"),
        ("mako.conf", "mako.conf"),
        ("kitty.conf", "kitty.conf"),
        ("gtk.css", "gtk.css"),
        ("apps.css", "apps.css"),
        ("overview.css", "overview.css"),
        ("gtklock.css", "gtklock.css"),
        ("swaync.css", "swaync.css"),
        ("swayosd.css", "swayosd.css"),
        ("nwg-bar.css", "nwg-bar.css"),
        ("shortcuts.css", "shortcuts.css"),
    ):
        write_atomic(theme_dir / target_name, render(template_dir / template_name, colors))

    gtk_css = render(template_dir / "gtk.css", colors)
    write_atomic(config_home / "gtk-3.0" / "gtk.css", gtk_css)
    write_atomic(config_home / "gtk-4.0" / "gtk.css", gtk_css)
    write_atomic(config_home / "swaync" / "style.css", render(template_dir / "swaync.css", colors))
    write_atomic(config_home / "swayosd" / "style.css", render(template_dir / "swayosd.css", colors))
    write_atomic(config_home / "tmux" / "arcos-theme.conf", render(template_dir / "tmux-theme.conf", colors))
    write_atomic(config_home / "nvim" / "arcos-theme.lua", render(template_dir / "nvim-theme.lua", colors))
    write_atomic(config_home / "walker" / "config.toml", (template_dir / "walker.toml").read_text(encoding="utf-8"))
    write_atomic(config_home / "walker" / "themes" / "arcos" / "style.css", render(template_dir / "walker.css", colors))
    # Voxtype reads the established Omarchy palette location. Mirroring the
    # live ArcOS palette there keeps its native waveform OSD in lockstep with
    # wallpaper-derived and manually selected accents.
    voxtype_colors = "\n".join(
        (
            f'background = "{colors["background"]}"',
            f'foreground = "{colors["foreground"]}"',
            f'accent = "{colors["accent"]}"',
            f'color1 = "{colors["critical"]}"',
            f'color2 = "{colors["accent_soft"]}"',
            f'color3 = "{colors["warning"]}"',
            "",
        )
    )
    write_atomic(config_home / "omarchy" / "current" / "theme" / "colors.toml", voxtype_colors)

    state = {
        "wallpaper": str(wallpaper),
        "accent": colors["accent"],
        "source_accent": accent,
        "source": source,
    }
    write_atomic(theme_dir / "theme.json", json.dumps(state, indent=2) + "\n")

    # Establish the icon/theme defaults on first login as well as during a live
    # reload. /etc/xdg provides a fallback, while dconf covers libadwaita apps.
    run_quiet(["gsettings", "set", "org.gnome.desktop.interface", "color-scheme", "prefer-dark"])
    run_quiet(["gsettings", "set", "org.gnome.desktop.interface", "gtk-theme", "adw-gtk3-dark"])
    run_quiet(["gsettings", "set", "org.gnome.desktop.interface", "icon-theme", "Papirus-Dark"])
    run_quiet(["gsettings", "set", "org.gnome.desktop.interface", "cursor-theme", "Bibata-Modern-Ice"])
    run_quiet(["gsettings", "set", "org.gnome.desktop.interface", "font-name", "JetBrainsMono Nerd Font 11"])
    # dconf writes make these defaults deterministic even in minimal sessions
    # where the full GNOME schema search path is not exported yet.
    for key, value in (
        ("color-scheme", "prefer-dark"),
        ("gtk-theme", "adw-gtk3-dark"),
        ("icon-theme", "Papirus-Dark"),
        ("cursor-theme", "Bibata-Modern-Ice"),
        ("font-name", "JetBrainsMono Nerd Font 11"),
    ):
        run_quiet(["dconf", "write", f"/org/gnome/desktop/interface/{key}", repr(value)])

    if reload_desktop:
        run_quiet(["swaymsg", f"output * bg {wallpaper} fill"])
        run_quiet(
            [
                "swaymsg",
                f"client.focused {colors['accent_soft']} {colors['background_alt']} "
                f"{colors['foreground']} {colors['accent_soft']} {colors['accent_soft']}",
            ]
        )
        run_quiet(
            [
                "systemctl",
                "--user",
                "kill",
                "--signal=SIGUSR2",
                "arc-waybar.service",
            ]
        )
        run_quiet(["makoctl", "reload"])
        run_quiet(["swaync-client", "--reload-config"])
        run_quiet(["swaync-client", "--reload-css"])
        run_quiet(["systemctl", "--user", "restart", "arc-swayosd.service"])
        run_quiet(["arcos-rgb-sync"])
    print(f"Theme applied: {colors['accent']} · {wallpaper}")


def load_state() -> tuple[Path, str, str]:
    config_home = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    state_path = config_home / "arcos-desktop" / "theme.json"
    default_wallpaper = Path(os.environ.get("ARCOS_DEFAULT_WALLPAPER", ""))
    if not state_path.is_file():
        return default_wallpaper, choose_accent(default_wallpaper), "wallpaper"
    state = json.loads(state_path.read_text(encoding="utf-8"))
    wallpaper = Path(state.get("wallpaper", default_wallpaper))
    source = state.get("source", "manual")
    accent = parse_color(state.get("source_accent", DEFAULT_ACCENT))
    if source == "wallpaper":
        accent = choose_accent(wallpaper)
    return wallpaper, accent, source


def main() -> None:
    command = sys.argv[1] if len(sys.argv) > 1 else "apply"
    wallpaper, accent, source = load_state()
    if command == "wallpaper":
        if len(sys.argv) != 3:
            fail("usage: arcos-theme wallpaper PATH")
        wallpaper = Path(sys.argv[2]).expanduser().resolve()
        accent = choose_accent(wallpaper)
        source = "wallpaper"
    elif command == "color":
        if len(sys.argv) != 3:
            fail("usage: arcos-theme color '#RRGGBB'")
        accent = parse_color(sys.argv[2])
        source = "manual"
    elif command == "auto":
        accent = choose_accent(wallpaper)
        source = "wallpaper"
    elif command == "ensure":
        apply_theme(wallpaper, accent, source, reload_desktop=False)
        return
    elif command == "status":
        print(json.dumps({"wallpaper": str(wallpaper), "source_accent": accent, "source": source}, indent=2))
        return
    elif command != "apply":
        fail("use: apply, auto, ensure, wallpaper PATH, color HEX, or status")
    apply_theme(wallpaper, accent, source)


if __name__ == "__main__":
    main()
