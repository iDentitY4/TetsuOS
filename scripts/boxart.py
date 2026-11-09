#!/usr/bin/env python3
"""Fetches box art for the games on the device, and copies it there over the USB link.

Covers come from the libretro-thumbnails collections on GitHub, which are named after
No-Intro file names like the games. They are shrunk to the launcher's art panel and saved
as plain PPM images, which the app reads without a PNG decoder, in /mnt/data/boxart on
the device. Downloads are cached in ~/.cache/rg35xxsp-boxart, outside the project,
because the covers are copyrighted artwork. Needs Pillow.

usage: scripts/boxart.py
"""
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

from PIL import Image

DEVICE = os.environ.get("RG_DEVICE", "root@10.35.35.1")
SSH = ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
       "-o", "LogLevel=ERROR"]
ROM_DIR = "/mnt/data/roms"
ART_DIR = "/mnt/data/boxart"
CACHE = os.path.expanduser("~/.cache/rg35xxsp-boxart")
# The art panel in the launcher's game lists.
MAX_SIZE = (200, 300)
COLLECTIONS = {
    "nes": "Nintendo_-_Nintendo_Entertainment_System",
    "gb": "Nintendo_-_Game_Boy",
    "gbc": "Nintendo_-_Game_Boy_Color",
    "gba": "Nintendo_-_Game_Boy_Advance",
    "sfc": "Nintendo_-_Super_Nintendo_Entertainment_System",
    "cue": "Sony_-_PlayStation",
    "chd": "Sony_-_PlayStation",
    "pbp": "Sony_-_PlayStation",
    "m3u": "Sony_-_PlayStation",
    "smc": "Nintendo_-_Super_Nintendo_Entertainment_System",
}
URL = "https://raw.githubusercontent.com/libretro-thumbnails/{}/master/Named_Boxarts/{}.png"
TREE_URL = "https://api.github.com/repos/libretro-thumbnails/{}/git/trees/master?recursive=1"


def thumbnail_name(stem):
    """libretro-thumbnails replaces characters that file systems disallow."""
    return "".join("_" if c in '&*/:`<>?\\|"' else c for c in stem)


PNG_SIGNATURE = b"\x89PNG"


def fetch(collection, stem, links=3):
    """The cover's PNG bytes, from the cache or GitHub. None if there is no cover.

    The collections hold alternative names as symbolic links, which GitHub serves as a
    file holding the target's name, so those are followed.
    """
    data = fetch_file(collection, thumbnail_name(stem))
    while data is not None and not data.startswith(PNG_SIGNATURE) and links > 0:
        target = data.decode("utf-8", "replace").strip()
        if not target.endswith(".png") or "/" in target:
            return None
        data = fetch_file(collection, target[: -len(".png")])
        links -= 1
    return data if data is not None and data.startswith(PNG_SIGNATURE) else None


def fetch_file(collection, name):
    """A file from a collection's covers, from the cache or GitHub. None if it is missing."""
    cached = os.path.join(CACHE, collection, name + ".png")
    missing = cached + ".missing"
    if os.path.exists(cached):
        with open(cached, "rb") as file:
            return file.read()
    if os.path.exists(missing):
        return None
    os.makedirs(os.path.dirname(cached), exist_ok=True)
    url = URL.format(collection, urllib.parse.quote(name))
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read()
    except urllib.error.HTTPError as error:
        if error.code == 404:
            open(missing, "w").close()
            return None
        raise
    with open(cached, "wb") as file:
        file.write(data)
    return data


def cover_names(collection):
    """All cover names in a collection, from one listing of its files, cached for a day."""
    cached = os.path.join(CACHE, collection, "covers.json")
    if os.path.exists(cached) and os.path.getmtime(cached) > time.time() - 86400:
        with open(cached) as file:
            return json.load(file)
    request = urllib.request.Request(TREE_URL.format(collection), headers={"Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(request, timeout=60) as response:
        tree = json.load(response)["tree"]
    prefix = "Named_Boxarts/"
    names = [item["path"][len(prefix):-len(".png")] for item in tree
             if item["path"].startswith(prefix) and item["path"].endswith(".png")]
    os.makedirs(os.path.dirname(cached), exist_ok=True)
    with open(cached, "w") as file:
        json.dump(names, file)
    return names


def title_and_tags(name):
    """Splits "Tetris DX (World) (SGB Enhanced)" into "tetris dx" and its tags."""
    title = name.partition(" (")[0]
    return title.lower(), [tag.lower() for tag in re.findall(r"\(([^)]*)\)", name)]


def closest_cover(collection, stem):
    """The cover with the same title, preferring the most tags in common, such as region."""
    title, tags = title_and_tags(stem)
    best, best_score = None, -1
    for name in cover_names(collection):
        other_title, other_tags = title_and_tags(name)
        if other_title != title:
            continue
        score = len(set(tags) & set(other_tags)) * 2 - len(set(other_tags) - set(tags))
        if score > best_score:
            best, best_score = name, score
    return best


def main():
    listing = subprocess.run(SSH + [DEVICE, f"ls {ROM_DIR}"], capture_output=True, text=True, check=True)
    games = [line for line in listing.stdout.splitlines() if line.strip()]
    with tempfile.TemporaryDirectory() as staging:
        found, missing = 0, []
        for game in games:
            stem, extension = os.path.splitext(game)
            collection = COLLECTIONS.get(extension[1:].lower())
            if collection is None:
                continue
            data = fetch(collection, stem)
            if data is None:
                # Names differ in tags between sets, like (GB Compatible); try the closest.
                closest = closest_cover(collection, stem)
                data = fetch(collection, closest) if closest else None
            if data is None:
                missing.append(game)
                continue
            image = Image.open(io.BytesIO(data)).convert("RGB")
            image.thumbnail(MAX_SIZE, Image.LANCZOS)
            image.save(os.path.join(staging, stem + ".ppm"))
            found += 1
        subprocess.run(SSH + [DEVICE, f"mkdir -p {ART_DIR}"], check=True)
        if found:
            subprocess.run(["rsync", "-rt", "-e", " ".join(SSH), staging + "/", f"{DEVICE}:{ART_DIR}/"], check=True)
    print(f"{found} covers copied to {ART_DIR}")
    for game in missing:
        print(f"no cover found: {game}")


if __name__ == "__main__":
    sys.exit(main())
