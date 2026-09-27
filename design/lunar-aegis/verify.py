#!/usr/bin/env python3
"""Verify the account-independent Lunar Aegis design export."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def local_path(relative: str) -> Path:
    path = (ROOT / relative).resolve()
    if not path.is_relative_to(ROOT):
        raise ValueError(f"Path escapes design profile: {relative}")
    return path


def check_hash(path: Path, expected: str) -> None:
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != expected:
        raise ValueError(f"SHA-256 mismatch: {path.relative_to(ROOT)}")


def main() -> None:
    profile = read_json(ROOT / "profile.json")
    manifest = read_json(ROOT / "screens/manifest.json")
    flow = read_json(ROOT / "flow.json")
    if {profile["id"], manifest["profile"], flow["profile"]} != {"lunar-aegis"}:
        raise ValueError("Profile identifiers differ")

    nodes = flow["nodes"]
    edges = flow["edges"]
    node_ids = {node["id"] for node in nodes}
    if len(node_ids) != len(nodes):
        raise ValueError("Duplicate flow node IDs")
    for edge in edges:
        if edge["from"] not in node_ids or edge["to"] not in node_ids:
            raise ValueError(f"Broken flow edge: {edge['id']}")

    screens = manifest["screens"]
    design_ids = {screen["designId"] for screen in screens}
    if len(design_ids) != len(screens):
        raise ValueError("Duplicate design IDs in manifest")
    linked_ids = {node["designId"] for node in nodes if node["designId"]}
    if design_ids != linked_ids:
        raise ValueError("Screen manifest and flow links differ")

    for screen in screens:
        html = local_path(screen["html"])
        image = local_path(screen["image"])
        check_hash(html, screen["sourceHtmlSha256"])
        check_hash(image, screen["imageSha256"])
        if not image.read_bytes().startswith(PNG_MAGIC):
            raise ValueError(f"Invalid PNG: {image.relative_to(ROOT)}")
        expected_nodes = {
            node["id"] for node in nodes if node["designId"] == screen["designId"]
        }
        if expected_nodes != set(screen["nodeIds"]):
            raise ValueError(f"Node mapping differs: {screen['designId']}")

    vector_hashes = read_json(ROOT / "assets/vector-sha256.json")
    expected_vectors = {"assets/logo-master.svg", *(
        entry["icon"] for entry in profile["navigation"]
    )}
    if set(vector_hashes) != expected_vectors:
        raise ValueError("Vector manifest and profile assets differ")
    for asset, digest in vector_hashes.items():
        check_hash(local_path(asset), digest)
    if len(profile["navigation"]) != 5:
        raise ValueError("Expected exactly five navigation entries")
    if [entry["id"] for entry in profile["navigation"]] != [
        "home", "wallet", "scan", "documents", "activity"
    ]:
        raise ValueError("Navigation order differs")

    for font in (
        "SpaceGrotesk[wght].ttf",
        "PlusJakartaSans[wght].ttf",
        "JetBrainsMono[wght].ttf",
        "NotoSans[wdth,wght].ttf",
    ):
        data = local_path(f"fonts/{font}").read_bytes()
        if data[:4] not in (b"\x00\x01\x00\x00", b"OTTO"):
            raise ValueError(f"Invalid font: {font}")

    print(
        f"lunar-aegis export verified: {len(screens)} screens, "
        f"{len(nodes)} flow nodes, {len(edges)} transitions, "
        f"{len(profile['navigation'])} navigation icons"
    )


if __name__ == "__main__":
    main()
