#!/usr/bin/env python3
"""OTA 用 .bin の先頭 4 kB にある IMAGE_DEF を、実機 (src/ota/image.rs) と同じ規則で検査する。

    scripts/check-image-def.py <bin> [--version X.Y.Z] [--allow-plain]

実機は 1.0.1 から、manifest の SHA-256 が合っていても IMAGE_DEF が
  * EXE / Secure / Arm / RP2350 で
  * TBYB フラグ (0x8000) が立っていて
  * VERSION 項目が manifest の version (minor × 100 + patch に畳む) と一致する
ときだけ他方区画に確定する。CI (build.yml) と make-manifest.sh はこのスクリプトで同じ検査をして、
条件を満たさない bin を Release の manifest に載せない。

終了コード: 0 = 合格、1 = 不合格 (理由を stderr に出す)、2 = 使い方の誤り。
"""

from __future__ import annotations

import argparse
import re
import struct
import sys
from pathlib import Path

SCAN_LEN = 4096
MARKER_START = 0xFFFFDED3
MARKER_END = 0xAB123579
ITEM_2BS_FLAG = 0x80
ITEM_KIND_MASK = 0x7F
ITEM_IMAGE_TYPE = 0x42
ITEM_VERSION = 0x48
ITEM_LAST = 0xFF
MAX_ITEM_WORDS = SCAN_LEN // 4

TYPE_EXE = 0x0001
TYPE_KIND_MASK = 0x0003
TYPE_SECURITY_MASK = 0x0030
TYPE_SECURITY_S = 0x0020
TYPE_CPU_MASK = 0x0300
TYPE_CPU_ARM = 0x0000
TYPE_CHIP_MASK = 0x7000
TYPE_CHIP_RP2350 = 0x1000
TYPE_TBYB = 0x8000


def word_at(scan: bytes, at: int) -> int | None:
    if at < 0 or at + 4 > len(scan):
        return None
    return struct.unpack_from("<I", scan, at)[0]


_NOT_A_BLOCK = object()


def parse_block_at(scan: bytes, at: int) -> dict | None | object:
    """src/ota/image.rs の parse_block_at と同じ。

    _NOT_A_BLOCK = ブロックとして整合しない (次の開始マーカーを試す)、None = 整合したが IMAGE_DEF と
    認めない (第 1 項目が IMAGE_TYPE でない、重複、リンクが 0 でない)、dict = IMAGE_DEF。
    """
    pos = at + 4
    words = 0
    image_type = None
    version = None
    acceptable = True
    while True:
        header = word_at(scan, pos)
        if header is None:
            return _NOT_A_BLOCK
        kind = header & 0xFF
        if kind == ITEM_LAST:
            total = (header >> 8) & 0xFFFF
            link = word_at(scan, pos + 4)
            if total != words or link is None or word_at(scan, pos + 8) != MARKER_END:
                return _NOT_A_BLOCK
            if image_type is None or not acceptable or link != 0:
                return None
            return {"image_type": image_type, "version": version}
        size = (header >> 8) & 0xFFFF if kind & ITEM_2BS_FLAG else (header >> 8) & 0xFF
        if size == 0 or words + size > MAX_ITEM_WORDS or pos + size * 4 > len(scan):
            return _NOT_A_BLOCK
        base = kind & ITEM_KIND_MASK
        is_1bs = not kind & ITEM_2BS_FLAG
        if base == ITEM_IMAGE_TYPE and is_1bs:
            if words != 0 or image_type is not None:
                acceptable = False
            image_type = (header >> 16) & 0xFFFF
        elif base == ITEM_VERSION and is_1bs and size >= 2:
            if version is not None:
                acceptable = False
            value = word_at(scan, pos + 4)
            if value is None:
                return _NOT_A_BLOCK
            version = (value >> 16, value & 0xFFFF)
        pos += size * 4
        words += size


def parse(first_sector: bytes) -> dict | None:
    """最初に整合したブロックを採用する (bootrom と同じ)。それが IMAGE_DEF でなければ None"""
    scan = first_sector[:SCAN_LEN]
    for at in range(0, len(scan) - 3, 4):
        if word_at(scan, at) == MARKER_START:
            found = parse_block_at(scan, at)
            if found is not _NOT_A_BLOCK:
                return found
    return None


def expected_version(text: str) -> tuple[int, int] | None:
    """src/ota/manifest.rs の Version::parse + src/ota/image.rs の expected_version"""
    if not re.fullmatch(r"v?[0-9]+\.[0-9]+\.[0-9]+", text):
        return None
    parts = text.lstrip("v").split(".")
    major, minor, patch = (int(p) for p in parts)
    if patch >= 100:
        return None
    folded = minor * 100 + patch
    if major > 0xFFFF or folded > 0xFFFF:
        return None
    return (major, folded)


def is_rp2350_arm_exe(image_type: int) -> bool:
    return (
        image_type & TYPE_KIND_MASK == TYPE_EXE
        and image_type & TYPE_SECURITY_MASK == TYPE_SECURITY_S
        and image_type & TYPE_CPU_MASK == TYPE_CPU_ARM
        and image_type & TYPE_CHIP_MASK == TYPE_CHIP_RP2350
    )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("bin", type=Path, help="make-ota-image.sh が作った .bin (objcopy -O binary)")
    ap.add_argument("--version", help="manifest に載せる X.Y.Z。IMAGE_DEF の VERSION と一致すること")
    ap.add_argument("--allow-plain", action="store_true", help="TBYB フラグが無くても合格にする (OTA で配らない bin 用)")
    args = ap.parse_args()

    try:
        with args.bin.open("rb") as f:
            head = f.read(SCAN_LEN)
    except OSError as e:
        print(f"error: {args.bin}: {e}", file=sys.stderr)
        return 2

    found = parse(head)
    if found is None:
        print(f"FAIL {args.bin}: no IMAGE_DEF in the first {SCAN_LEN} bytes", file=sys.stderr)
        return 1
    image_type = found["image_type"]
    version = found["version"]
    tbyb = bool(image_type & TYPE_TBYB)
    shown = f"{version[0]}.{version[1]}" if version else "(none)"
    print(f"{args.bin}: image type 0x{image_type:04x} tbyb={'yes' if tbyb else 'no'} version={shown}")

    if not is_rp2350_arm_exe(image_type):
        print(f"FAIL {args.bin}: not an RP2350 Arm Secure EXE image", file=sys.stderr)
        return 1
    if not tbyb and not args.allow_plain:
        print(f"FAIL {args.bin}: TBYB flag missing (build with --features tbyb)", file=sys.stderr)
        return 1
    if args.version is not None:
        want = expected_version(args.version)
        if want is None:
            print(f"FAIL {args.bin}: version {args.version!r} cannot be carried by IMAGE_DEF (X.Y.Z, patch < 100)", file=sys.stderr)
            return 1
        if version is None:
            print(f"FAIL {args.bin}: IMAGE_DEF has no VERSION item", file=sys.stderr)
            return 1
        if version != want:
            print(f"FAIL {args.bin}: IMAGE_DEF version {shown} != manifest {args.version} (= {want[0]}.{want[1]})", file=sys.stderr)
            return 1
    print(f"OK {args.bin}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
