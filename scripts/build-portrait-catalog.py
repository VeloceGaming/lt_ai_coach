#!/usr/bin/env python3
"""Build base champion portrait metadata directly from TFM2 bundle.game_data."""

import argparse
import json
import os
import struct
from pathlib import Path

CHAMPION_PREFIX = "asset/base/aseprite_resources/champions/"
CHAMPION_VIEW_PATH = "asset/base/style/champion_view"


def records(bundle: Path):
    data = bundle.read_bytes()
    offset = 4
    while offset + 4 <= len(data):
        ext_len = struct.unpack_from("<I", data, offset)[0]
        offset += 4
        ext = data[offset : offset + ext_len].decode("latin1")
        offset += ext_len
        path_len = struct.unpack_from("<I", data, offset)[0]
        offset += 4
        path = data[offset : offset + path_len].decode("latin1")
        offset += path_len
        data_len = struct.unpack_from("<I", data, offset)[0]
        offset += 4
        payload = data[offset : offset + data_len]
        offset += data_len
        yield ext, path, payload


def png_size(payload: bytes):
    if len(payload) < 24 or payload[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    return struct.unpack_from(">II", payload, 16)


def first_portrait_frame(payload: bytes):
    document = json.loads(payload.decode("utf-8"))
    animations = document.get("anims") or {}
    animation_name = "idle" if animations.get("idle", {}).get("frames") else None
    if animation_name is None:
        animation_name = next(
            (name for name, animation in animations.items() if animation.get("frames")),
            None,
        )
    if animation_name is None:
        return None
    frame = animations[animation_name]["frames"][0].get("data") or {}
    if not all(key in frame for key in ("x", "y", "w", "h")):
        return None
    return {
        "animation": animation_name,
        "index": 0,
        "x": frame["x"],
        "y": frame["y"],
        "width": frame["w"],
        "height": frame["h"],
    }


def default_bundle():
    candidates = []
    local = os.environ.get("LOCALAPPDATA")
    if local:
        config = Path(local) / "TFM2Forge" / "config.json"
        if config.is_file():
            try:
                candidates.append(Path(json.loads(config.read_text(encoding="utf-8"))["bundle"]))
            except (KeyError, ValueError, OSError):
                pass
    candidates.extend(
        [
            Path(r"C:\Program Files (x86)\Steam\steamapps\common\Teamfight Manager2\bundle.game_data"),
            Path(r"C:\Program Files (x86)\Steam\steamapps\common\Teamfight Manager 2\bundle.game_data"),
        ]
    )
    return next((path for path in candidates if path.is_file()), None)


def build(bundle: Path, assets_output: Path):
    sheets = {}
    frames = {}
    view_entries = {}
    for extension, asset_path, payload in records(bundle):
        if asset_path == CHAMPION_VIEW_PATH:
            parsed = json.loads(payload.decode("utf-8"))
            view_entries = parsed.get("entries", parsed) or {}
        elif asset_path.startswith(CHAMPION_PREFIX):
            relative = asset_path[len(CHAMPION_PREFIX) :]
            sprite = relative.split("#", 1)[0]
            if "#sheet" in relative and extension == "png":
                size = png_size(payload)
                if size:
                    sheets[sprite] = (size, payload)
            elif "#anim" in relative and extension == "fanim":
                try:
                    frame = first_portrait_frame(payload)
                except (UnicodeDecodeError, ValueError, TypeError):
                    frame = None
                if frame:
                    frames[sprite] = frame

    sprite_ids = sorted(set(sheets) | set(frames) | set(view_entries))
    portraits = {}
    warnings = []
    for sprite in sprite_ids:
        sheet = sheets.get(sprite)
        size = sheet[0] if sheet else None
        view = view_entries.get(sprite) or {}
        frame = frames.get(sprite)
        missing = []
        if not size:
            missing.append("sheet")
        if not frame:
            missing.append("frame")
        if sprite not in view_entries:
            missing.append("viewOffsets")
        if missing:
            warnings.append({"sprite": sprite, "missing": missing})
        if sheet:
            assets_output.mkdir(parents=True, exist_ok=True)
            (assets_output / f"{sprite}.png").write_bytes(sheet[1])
        portraits[sprite] = {
            "sheet": f"assets/generated/base-portraits/{sprite}.png" if sheet else None,
            "sheetAsset": f"{CHAMPION_PREFIX}{sprite}#sheet.png",
            "sheetWidth": size[0] if size else None,
            "sheetHeight": size[1] if size else None,
            "frame": frame,
            "faceOffset": view.get("face", {"x": 0, "y": 0}),
            "centerOffset": view.get("center", {"x": 0, "y": 0}),
            "missing": missing,
        }
    return {
        "schemaVersion": 1,
        "generatedFrom": "Teamfight Manager 2 bundle.game_data",
        "selectionRule": "idle.frames[0], otherwise first non-empty animation frame",
        "portraits": portraits,
        "warnings": warnings,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "data" / "catalog" / "portraits.json",
    )
    parser.add_argument("--assets-output", type=Path, default=Path(__file__).resolve().parents[1] / "assets" / "generated" / "base-portraits")
    args = parser.parse_args()
    bundle = args.bundle or default_bundle()
    if bundle is None or not bundle.is_file():
        parser.error("bundle.game_data was not found; pass --bundle <path>")
    result = build(bundle, args.assets_output)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Wrote {len(result['portraits'])} portrait records to {args.output}")
    print(f"Records with missing metadata: {len(result['warnings'])}")


if __name__ == "__main__":
    main()
