#!/usr/bin/env python3
"""
Generate src-tauri/src/transcription/t2s_table.rs from OpenCC's
TSCharacters.txt mapping (Apache-2.0 licensed).

Usage:
    python3 scripts/gen_t2s_table.py [--source URL_OR_PATH]

By default fetches the latest data from BYVoid/OpenCC on GitHub. Pass a local
path to use a cached copy. The resulting Rust file contains a sorted slice of
`(char, char)` pairs used by `convert_to_simplified()` via binary search.

This script is the single source of truth for the conversion table — do not
hand-edit `t2s_table.rs`. Re-run this whenever you want to refresh the mapping.
"""

from __future__ import annotations

import argparse
import os
import sys
from urllib.request import urlopen

DEFAULT_SOURCE = (
    "https://raw.githubusercontent.com/BYVoid/OpenCC/master/data/dictionary/TSCharacters.txt"
)
OUTPUT_PATH = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "src-tauri",
    "src",
    "transcription",
    "t2s_table.rs",
)


def read_source(source: str) -> str:
    if source.startswith(("http://", "https://")):
        with urlopen(source) as resp:
            return resp.read().decode("utf-8")
    with open(source, "r", encoding="utf-8") as fh:
        return fh.read()


def parse_pairs(text: str) -> list[tuple[int, int]]:
    pairs: list[tuple[int, int]] = []
    for line in text.splitlines():
        line = line.rstrip("\n")
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 2:
            continue
        key = parts[0]
        targets = parts[1].split(" ")
        target = targets[0]
        if not key or not target:
            continue
        if len(key) != 1 or len(target) != 1:
            # Multi-codepoint sources/targets are rare; keep the first char only
            if len(target) > 1:
                target = target[0]
            if len(key) != 1:
                continue
        if key == target:
            continue
        pairs.append((ord(key), ord(target)))

    # Sort and dedupe by source
    pairs.sort(key=lambda p: p[0])
    seen: set[int] = set()
    unique: list[tuple[int, int]] = []
    for p in pairs:
        if p[0] in seen:
            continue
        seen.add(p[0])
        unique.append(p)
    return unique


def emit_rust(pairs: list[tuple[int, int]]) -> str:
    out: list[str] = []
    out.append("// Auto-generated from OpenCC TSCharacters.txt (Apache-2.0 License).")
    out.append(
        "// Source: https://github.com/BYVoid/OpenCC/blob/master/data/dictionary/TSCharacters.txt"
    )
    out.append("// DO NOT EDIT BY HAND. Regenerate via scripts/gen_t2s_table.py.")
    out.append("//")
    out.append(f"// Total entries: {len(pairs)}")
    out.append("// Sorted by Traditional (source) codepoint to enable binary search.")
    out.append("")
    out.append("pub(super) static T2S_TABLE: &[(char, char)] = &[")

    per_line = 6
    buf: list[str] = []
    for src, dst in pairs:
        buf.append(f"('\\u{{{src:X}}}', '\\u{{{dst:X}}}')")
        if len(buf) == per_line:
            out.append("    " + ", ".join(buf) + ",")
            buf = []
    if buf:
        out.append("    " + ", ".join(buf) + ",")
    out.append("];")
    out.append("")
    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--source",
        default=DEFAULT_SOURCE,
        help="URL or local path to OpenCC TSCharacters.txt",
    )
    ap.add_argument("--output", default=OUTPUT_PATH)
    args = ap.parse_args()

    print(f"Reading source: {args.source}", file=sys.stderr)
    raw = read_source(args.source)
    pairs = parse_pairs(raw)
    if not pairs:
        print("No pairs parsed — aborting", file=sys.stderr)
        return 1

    rust = emit_rust(pairs)
    with open(args.output, "w", encoding="utf-8") as fh:
        fh.write(rust)
    print(
        f"Wrote {len(pairs)} entries to {args.output}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
