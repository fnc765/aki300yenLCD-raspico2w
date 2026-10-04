"""Read USB logs only from the explicitly identified Pico; never an unrelated COM port."""
import argparse
from datetime import datetime
from pathlib import Path
import time
import serial
from serial.tools import list_ports

parser = argparse.ArgumentParser()
parser.add_argument("--serial", required=True)
parser.add_argument("--seconds", type=int, default=45)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--no-start", action="store_true", help="Observe USB before starting Wi-Fi/Matter.")
args = parser.parse_args()
deadline = time.monotonic() + 20
port = None
while time.monotonic() < deadline:
    found = [p for p in list_ports.comports()
             if p.vid == 0x2E8A and p.pid == 0x000A
             and (p.serial_number or "").upper() == args.serial.upper()]
    if len(found) == 1:
        port = found[0].device
        break
    if len(found) > 1:
        raise SystemExit("Ambiguous device identity; no port opened.")
    time.sleep(0.5)
if port is None:
    raise SystemExit("Identified Pico USB serial interface not found; no port opened.")
print(f"USB_CAPTURE port={port} serial={args.serial}", flush=True)
with serial.Serial(port, 115200, timeout=0.5) as connection, args.output.open("a", encoding="utf-8") as output:
    if not args.no_start:
        connection.write(b"START\n")
    deadline = time.monotonic() + args.seconds
    while time.monotonic() < deadline:
        text = connection.readline().decode("utf-8", errors="replace").rstrip()
        if text:
            line = f"{datetime.now().isoformat(timespec='seconds')} {text}"
            print(line, flush=True)
            output.write(line + "\n")
            output.flush()
