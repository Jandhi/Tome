#!/usr/bin/env python3
"""Generate (or refresh) the JSON sidecar for one or more Minecraft NBT structures.

For each NBT file, this reads the structure's `size` tag and writes a sibling
`.json` describing it for Tome's `Structure` loader:

  * `size_xz`  -> [size_x, size_z] from the NBT bounding box
  * `origin`   -> the structure's center, floor-divided ((size-1)//2)
  * `y_offset` -> blocks the structure extends below ground (NOT derivable from
                  the NBT; defaults to 0, preserved from an existing JSON)
  * `id`       -> the file stem
  * `path`     -> repo-relative path with forward slashes

If the JSON already exists, only `size_xz` and `origin` are recomputed; all
other fields (y_offset, palette, facing, tags, weight, ...) are kept as-is.

Usage:
  python scripts/gen_nbt_json.py path/to/structure.nbt [more.nbt ...]
  python scripts/gen_nbt_json.py data/structures/resource_buildings/*.nbt

Requires: nbtlib  (pip install nbtlib)
"""
import json
import sys
from pathlib import Path

try:
    import nbtlib
except ImportError:
    sys.exit("error: nbtlib is required -> pip install nbtlib")

# Repo root = parent of the scripts/ directory this file lives in.
REPO_ROOT = Path(__file__).resolve().parent.parent


def repo_relative(path: Path) -> str:
    """Path relative to the repo root, using forward slashes (matches loader)."""
    try:
        rel = path.resolve().relative_to(REPO_ROOT)
    except ValueError:
        rel = path  # outside the repo; fall back to whatever was given
    return rel.as_posix()


def generate(nbt_path: Path) -> None:
    nbt = nbtlib.load(nbt_path)
    size = [int(v) for v in nbt["size"]]  # [x, y, z]
    if len(size) != 3 or any(s <= 0 for s in size):
        raise ValueError(f"{nbt_path.name}: unexpected size tag {size}")
    sx, _sy, sz = size

    json_path = nbt_path.with_suffix(".json")
    if json_path.exists():
        with json_path.open(encoding="utf-8") as f:
            data = json.load(f)
    else:
        data = {"id": nbt_path.stem, "y_offset": 0}

    data["id"] = data.get("id", nbt_path.stem)
    data["path"] = repo_relative(nbt_path)
    data["origin"] = {"x": (sx - 1) // 2, "y": 0, "z": (sz - 1) // 2}
    data["size_xz"] = [sx, sz]
    data.setdefault("y_offset", 0)

    with json_path.open("w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
        f.write("\n")

    print(f"{json_path.name}: size_xz={data['size_xz']} origin="
          f"({data['origin']['x']},{data['origin']['y']},{data['origin']['z']}) "
          f"y_offset={data['y_offset']}")


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 1
    rc = 0
    for arg in argv:
        path = Path(arg)
        if not path.is_file():
            print(f"skip: {arg} (not a file)", file=sys.stderr)
            rc = 1
            continue
        try:
            generate(path)
        except Exception as e:  # noqa: BLE001 - report and continue
            print(f"error: {path.name}: {e}", file=sys.stderr)
            rc = 1
    return rc


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
