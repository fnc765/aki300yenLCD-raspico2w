#!/usr/bin/env python3
"""Export the existing probe identity to PRIVATE SD files; never prints secrets."""
import argparse
import subprocess
from pathlib import Path
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--probe", type=Path, default=Path("experiments/pico2w-matter-probe"))
parser.add_argument("--out", type=Path, default=Path(".local/ticker-provision"))
args = parser.parse_args()
seed = (args.probe / "controller-seed.local.bin").read_bytes()
if len(seed) != 32 or not any(seed):
    parser.error("the commissioned probe's 32-byte private identity is required")
wifi = tomllib.loads((args.probe / "probe.local.toml").read_text(encoding="utf-8-sig"))["wifi"]
ssid, password = wifi["ssid"], wifi["password"]
if not 1 <= len(ssid.encode()) <= 32 or any(c in ssid + password for c in "\r\n"):
    parser.error("invalid Wi-Fi settings")
if password and not 8 <= len(password.encode()) <= 63:
    parser.error("invalid Wi-Fi password length")
# Keep generated keys/firmware out of tracked files by requiring ignored output.
if subprocess.run(["git", "check-ignore", "-q", str(args.out / "MATTER.TXT")]).returncode:
    parser.error("--out must be inside a git-ignored directory")
args.out.mkdir(parents=True, exist_ok=True)
(args.out / "WIFI.TXT").write_bytes(f"{ssid}\n{password}\n".encode())
(args.out / "MATTER.TXT").write_bytes(f"controller_seed={seed.hex()}\ndevice_node=0x110\n".encode())
print(f"Private SD configuration prepared in {args.out}. Keep this directory and local firmware private.")
