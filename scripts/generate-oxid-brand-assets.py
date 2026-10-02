#!/usr/bin/env python3
"""Generate the Oxid logo family and bundled platform icons from one mark."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import cairosvg
from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
BRAND = ROOT / "brands/oxid/assets"
ICONS = ROOT / "apps/oxid/icons"

# Four ridges form a crescent-shaped fingerprint. Keep this geometry identical
# in the inline mark, dark icon, light icon, and desktop bundle artwork.
RIDGES = (
    ("M316 114 C263 102 205 122 159 164 C111 208 92 264 106 320 C120 383 180 422 248 428 C273 431 297 426 316 418", 34),
    ("M349 178 C311 153 268 161 233 190 C197 220 181 260 187 302 C189 317 195 331 204 343", 31),
    ("M375 263 C357 228 329 214 301 216 C269 218 246 241 239 271 C231 302 244 329 269 348", 30),
    ("M353 351 C365 324 356 291 339 274 C325 260 305 258 290 269 C274 281 272 301 285 315", 30),
)


def paths(stroke: str) -> str:
    return "\n".join(
        f'  <path d="{path}" fill="none" stroke="{stroke}" stroke-width="{width}" '
        'stroke-linecap="round" stroke-linejoin="round"/>'
        for path, width in RIDGES
    )


def svg(body: str, view_box: str = "0 0 1024 1024") -> str:
    return (
        "<!-- SPDX-License-Identifier: Apache-2.0 -->\n"
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}" role="img" '
        'aria-label="Oxid fingerprint crescent">\n'
        f"{body}\n</svg>\n"
    )


def mark_group(stroke: str) -> str:
    return f'<g transform="translate(35 -8) scale(2)">\n{paths(stroke)}\n</g>'


def icon_body(light: bool, rounded: bool = False, android: bool = False) -> str:
    if light:
        definitions = """<defs>
  <linearGradient id="tile" x1="0%" y1="0%" x2="100%" y2="100%">
    <stop stop-color="#FCFEFF"/><stop offset="1" stop-color="#E8EDF4"/>
  </linearGradient>
  <linearGradient id="oval" x1="0%" y1="0%" x2="100%" y2="100%">
    <stop stop-color="#FFFFFF"/><stop offset="1" stop-color="#EDF3F8"/>
  </linearGradient>
  <linearGradient id="mark" x1="0%" y1="0%" x2="100%" y2="100%">
    <stop stop-color="#0369A1"/><stop offset="0.52" stop-color="#06B6D4"/>
    <stop offset="1" stop-color="#6840BD"/>
  </linearGradient>
</defs>"""
        oval = '<ellipse cx="512" cy="504" rx="346" ry="418" fill="url(#oval)" stroke="#D4E0F1" stroke-width="3"/>'
    else:
        definitions = """<defs>
  <linearGradient id="tile" x1="0%" y1="0%" x2="100%" y2="100%">
    <stop stop-color="#0F172A"/><stop offset="0.52" stop-color="#07090E"/>
    <stop offset="1" stop-color="#22113E"/>
  </linearGradient>
  <linearGradient id="oval" x1="0%" y1="0%" x2="100%" y2="100%">
    <stop stop-color="#38BDF8" stop-opacity="0.42"/>
    <stop offset="0.55" stop-color="#0F172A" stop-opacity="0.70"/>
    <stop offset="1" stop-color="#8B5CF6" stop-opacity="0.55"/>
  </linearGradient>
  <linearGradient id="mark" x1="0%" y1="0%" x2="100%" y2="100%">
    <stop stop-color="#A5F3FC"/><stop offset="0.43" stop-color="#06B6D4"/>
    <stop offset="1" stop-color="#8B5CF6"/>
  </linearGradient>
</defs>"""
        oval = '<ellipse cx="512" cy="504" rx="346" ry="418" fill="url(#oval)" stroke="#38BDF8" stroke-opacity="0.27" stroke-width="3"/>'
    tile = (
        '<rect x="30" y="30" width="964" height="964" rx="218" fill="url(#tile)"/>'
        if rounded
        else '<rect width="1024" height="1024" fill="url(#tile)"/>'
    )
    content = f"{oval}\n{mark_group('url(#mark)')}"
    if android:
        # Keep the complete mark within Android's central 66/108 safe region.
        content = f'<g transform="translate(143.36 143.36) scale(0.72)">\n{content}\n</g>'
    return f"{definitions}\n{tile}\n{content}"


def write_sources() -> None:
    BRAND.mkdir(parents=True, exist_ok=True)
    BRAND.joinpath("logo.svg").write_text(
        svg(paths("currentColor"), "48 80 410 390"), encoding="utf-8"
    )
    BRAND.joinpath("mark-gradient.svg").write_text(
        svg(
            '<defs><linearGradient id="mark" x1="0%" y1="0%" x2="100%" y2="100%">'
            '<stop stop-color="#A5F3FC"/><stop offset="0.43" stop-color="#06B6D4"/>'
            '<stop offset="1" stop-color="#8B5CF6"/></linearGradient></defs>\n'
            + paths("url(#mark)"),
            "48 80 410 390",
        ),
        encoding="utf-8",
    )
    BRAND.joinpath("app-icon-dark.svg").write_text(svg(icon_body(False)), encoding="utf-8")
    BRAND.joinpath("app-icon-light.svg").write_text(svg(icon_body(True)), encoding="utf-8")
    BRAND.joinpath("app-icon-android.svg").write_text(svg(icon_body(False, android=True)), encoding="utf-8")
    BRAND.joinpath("desktop-icon.svg").write_text(svg(icon_body(False, rounded=True)), encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL)


def render(source: Path, target: Path, size: int) -> None:
    cairosvg.svg2png(url=str(source), write_to=str(target),
                     output_width=size, output_height=size)
    with Image.open(target) as rendered:
        rendered.convert("RGBA").save(target)


def write_platform_icons() -> None:
    if not shutil.which("magick"):
        raise SystemExit("ImageMagick (magick) is required to package the Windows icon")
    ICONS.mkdir(parents=True, exist_ok=True)
    for name, source in [
        ("app-icon-dark-1024.png", "app-icon-dark.svg"),
        ("app-icon-light-1024.png", "app-icon-light.svg"),
        ("app-icon-android-1024.png", "app-icon-android.svg"),
    ]:
        render(BRAND / source, ICONS / name, 1024)
    for size in (16, 32, 64, 128, 256, 512):
        render(BRAND / "desktop-icon.svg", ICONS / f"desktop-{size}.png", size)
    run("magick", *(str(ICONS / f"desktop-{size}.png") for size in (16, 32, 64, 256)),
        str(ICONS / "icon.ico"))
    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as temporary:
            iconset = Path(temporary) / "Oxid.iconset"
            iconset.mkdir()
            for point_size in (16, 32, 128, 256, 512):
                render(BRAND / "desktop-icon.svg", iconset / f"icon_{point_size}x{point_size}.png", point_size)
                render(BRAND / "desktop-icon.svg", iconset / f"icon_{point_size}x{point_size}@2x.png", point_size * 2)
            run("iconutil", "-c", "icns", str(iconset), "-o", str(ICONS / "icon.icns"))


if __name__ == "__main__":
    write_sources()
    write_platform_icons()
