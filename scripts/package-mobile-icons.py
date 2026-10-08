#!/usr/bin/env python3
"""Install Oxid launcher artwork into Dioxus-generated native projects."""

from __future__ import annotations

import argparse
import plistlib
import shutil
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "apps/oxid/icons"
IOS_SOURCE = ICONS / "app-icon-dark-1024.png"
ANDROID_SOURCE = ICONS / "app-icon-android-1024.png"
ANDROID_ICON_NAMES = ("ic_launcher.png", "ic_launcher_round.png", "ic_launcher_foreground.png")


def image(source: Path, target: Path, size: int, opaque: bool = False) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    with Image.open(source) as opened:
        rendered = opened.resize((size, size), Image.Resampling.LANCZOS)
        rendered.convert("RGB" if opaque else "RGBA").save(target)


def package_ios(bundle: Path) -> None:
    if not bundle.is_dir():
        raise SystemExit(f"Dioxus did not create the expected iOS app bundle: {bundle}")
    with Image.open(IOS_SOURCE) as source:
        if source.mode == "RGBA" or "transparency" in source.info:
            raise SystemExit("the iOS launcher source must be opaque")
    icon_files = {
        "AppIcon20x20@2x.png": 40,
        "AppIcon20x20@3x.png": 60,
        "AppIcon29x29@2x.png": 58,
        "AppIcon29x29@3x.png": 87,
        "AppIcon40x40@2x.png": 80,
        "AppIcon40x40@3x.png": 120,
        "AppIcon60x60@2x.png": 120,
        "AppIcon60x60@3x.png": 180,
        "AppIcon76x76@2x.png": 152,
        "AppIcon83.5x83.5@2x.png": 167,
    }
    for name, size in icon_files.items():
        image(IOS_SOURCE, bundle / name, size, opaque=True)
    plist_path = bundle / "Info.plist"
    with plist_path.open("rb") as handle:
        plist = plistlib.load(handle)
    primary = {"CFBundleIconFiles": list(icon_files)}
    plist["CFBundleIcons"] = {"CFBundlePrimaryIcon": primary}
    plist["CFBundleIcons~ipad"] = {"CFBundlePrimaryIcon": primary}
    with plist_path.open("wb") as handle:
        plistlib.dump(plist, handle, sort_keys=False)


def package_android(project: Path) -> None:
    resource_root = project / "app/src/main/res"
    if not resource_root.is_dir():
        raise SystemExit(f"Dioxus did not create Android resources: {resource_root}")
    densities = {"mipmap-mdpi": 48, "mipmap-hdpi": 72, "mipmap-xhdpi": 96,
                 "mipmap-xxhdpi": 144, "mipmap-xxxhdpi": 192}
    for directory, size in densities.items():
        for name in ANDROID_ICON_NAMES:
            stem = Path(name).stem
            for generated in (resource_root / directory).glob(f"{stem}.*"):
                generated.unlink()
            image(ANDROID_SOURCE, resource_root / directory / name, size)
    adaptive = resource_root / "mipmap-anydpi-v26"
    adaptive.mkdir(parents=True, exist_ok=True)
    launcher = """<?xml version=\"1.0\" encoding=\"utf-8\"?>
<adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">
  <background android:drawable=\"@android:color/black\" />
  <foreground android:drawable=\"@mipmap/ic_launcher_foreground\" />
</adaptive-icon>
"""
    for name in ("ic_launcher.xml", "ic_launcher_round.xml"):
        (adaptive / name).write_text(launcher, encoding="utf-8")


def clean_android(project: Path) -> None:
    """Remove only launcher files owned by this packager before Dioxus regenerates."""
    resource_root = project / "app/src/main/res"
    if not resource_root.is_dir():
        return
    for directory in resource_root.glob("mipmap-*"):
        for name in ANDROID_ICON_NAMES:
            (directory / name).unlink(missing_ok=True)
    adaptive = resource_root / "mipmap-anydpi-v26"
    for name in ("ic_launcher.xml", "ic_launcher_round.xml"):
        (adaptive / name).unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("platform", choices=("ios", "android", "android-clean"))
    parser.add_argument("path", type=Path)
    arguments = parser.parse_args()
    if arguments.platform == "ios":
        package_ios(arguments.path)
    elif arguments.platform == "android":
        package_android(arguments.path)
    else:
        clean_android(arguments.path)


if __name__ == "__main__":
    main()
