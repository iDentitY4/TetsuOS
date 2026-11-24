#!/usr/bin/env python3
"""Draw the launcher's pixel icons as SVG files for the docs site.

Reads the 32x24 icons and their palette from src/launcher/src/ui/icons.rs, so the site
shows the same pictures as the device. Run it again after changing an icon:

    scripts/docs-icons.py            # the icons the home page uses
    scripts/docs-icons.py WIFI DOOM  # others, by their name in icons.rs
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICONS_RS = ROOT / "src/launcher/src/ui/icons.rs"
OUT = ROOT / "docs/public/icons"

# The home page's feature tiles
DEFAULT = ["GAME_BOY_ADVANCE", "RECENT", "WIFI", "ACHIEVEMENTS", "BATTERY", "SYSTEM"]


def parse(source):
    palette = {
        char: "#{:02x}{:02x}{:02x}".format(*(int(v, 16) for v in rgb))
        for char, *rgb in re.findall(
            r"'(\w)' => Rgb\(0x(\w\w), 0x(\w\w), 0x(\w\w)\)", source
        )
    }
    icons = {
        name: re.findall(r'"([^"]+)"', body)
        for name, body in re.findall(r"pub const (\w+): Icon = \[(.*?)\];", source, re.S)
    }
    return palette, icons


def svg(rows, palette):
    width, height = len(rows[0]), len(rows)
    rects = []
    for y, row in enumerate(rows):
        x = 0
        while x < width:
            char = row[x]
            run = 1
            while x + run < width and row[x + run] == char:
                run += 1
            if char in palette:
                rects.append(f'<rect x="{x}" y="{y}" width="{run}" height="1" fill="{palette[char]}"/>')
            x += run
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
        f'width="{width * 2}" height="{height * 2}" shape-rendering="crispEdges">'
        + "".join(rects)
        + "</svg>\n"
    )


def main():
    palette, icons = parse(ICONS_RS.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    for name in sys.argv[1:] or DEFAULT:
        if name not in icons:
            sys.exit(f"no icon named {name} in {ICONS_RS.relative_to(ROOT)}")
        path = OUT / f"{name.lower().replace('_', '-')}.svg"
        path.write_text(svg(icons[name], palette))
        print(path.relative_to(ROOT))


if __name__ == "__main__":
    main()
