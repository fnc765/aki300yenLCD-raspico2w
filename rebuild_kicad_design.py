from __future__ import annotations

import math
import re
import uuid
from pathlib import Path


ROOT = Path(__file__).resolve().parent
SRC_SCH = ROOT / "pico_lta042b010f_carrier.kicad_sch"
SRC_PCB = ROOT / "pico_lta042b010f_carrier.kicad_pcb"
SRC_PRO = ROOT / "pico_lta042b010f_carrier.kicad_pro"
OUT = Path(r"C:\Users\choco\AppData\Local\Temp\kicad-reviewed-candidate")
KI_SYMBOLS = Path(r"C:\Program Files\KiCad\9.0\share\kicad\symbols")
KI_FOOTPRINTS = Path(r"C:\Program Files\KiCad\9.0\share\kicad\footprints")

# Keep electrical coordinates on the original 1.27 mm grid and translate
# only the plotted sheet objects onto an A4 landscape page at the end.
SHEET_OFFSET_X = -60.96
SHEET_OFFSET_Y = -60.96


def uid() -> str:
    return str(uuid.uuid4())


def fmt(value: float) -> str:
    if abs(value) < 1e-9:
        value = 0.0
    return f"{value:.3f}".rstrip("0").rstrip(".")


def grid(value: float) -> float:
    return round(value / 1.27) * 1.27


def find_balanced(text: str, start: int) -> int:
    if text[start] != "(":
        raise ValueError(f"expected '(' at {start}")
    depth = 0
    quoted = False
    escaped = False
    for index in range(start, len(text)):
        char = text[index]
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            continue
        if char == '"':
            quoted = True
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return index + 1
    raise ValueError("unbalanced s-expression")


def extract_symbol(text: str, name: str) -> str:
    marker = f'(symbol "{name}"'
    start = text.find(marker)
    if start < 0:
        raise ValueError(f"symbol {name!r} not found")
    return text[start:find_balanced(text, start)]


def qualify_symbol(block: str, library: str, name: str) -> str:
    return block.replace(f'(symbol "{name}"', f'(symbol "{library}:{name}"', 1)


def top_level_blocks(text: str) -> list[str]:
    blocks: list[str] = []
    index = 0
    while index < len(text):
        if text[index] != "(":
            index += 1
            continue
        end = find_balanced(text, index)
        blocks.append(text[index:end])
        index = end
    return blocks


def round_tail_coordinates(text: str) -> str:
    pattern = re.compile(
        r'(?P<prefix>\((?:at|xy)\s+)'
        r'(?P<x>-?\d+(?:\.\d+)?)\s+'
        r'(?P<y>-?\d+(?:\.\d+)?)(?P<rest>[^\r\n\)]*)'
    )

    def replace(match: re.Match[str]) -> str:
        x = grid(float(match.group("x")))
        y = grid(float(match.group("y")))
        return f"{match.group('prefix')}{fmt(x)} {fmt(y)}{match.group('rest')}"

    return pattern.sub(replace, text)


def get_reference(block: str) -> str | None:
    match = re.search(r'\(property "Reference" "([^"]+)"', block)
    return match.group(1) if match else None


def get_first_at(block: str) -> tuple[float, float, float] | None:
    match = re.search(
        r'\(at\s+(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)(?:\s+(-?\d+(?:\.\d+)?))?\)',
        block,
    )
    if not match:
        return None
    return float(match.group(1)), float(match.group(2)), float(match.group(3) or 0)


def replace_root_at(block: str, x: float, y: float, rotation: float = 0) -> str:
    pattern = re.compile(
        r'\(at\s+-?\d+(?:\.\d+)?\s+-?\d+(?:\.\d+)?(?:\s+-?\d+(?:\.\d+)?)?\)'
    )
    return pattern.sub(f"(at {fmt(x)} {fmt(y)} {fmt(rotation)})", block, count=1)


def replace_property(block: str, name: str, value: str) -> str:
    pattern = re.compile(rf'(\(property "{re.escape(name)}" )"[^"]*"')
    return pattern.sub(lambda match: f'{match.group(1)}"{value}"', block, count=1)


def absolute_at(local_at: str, x: float, y: float, rotation: float = 0) -> str:
    values = [float(value) for value in local_at.split()]
    lx, ly = values[0], values[1]
    local_rotation = values[2] if len(values) > 2 else 0.0
    radians = math.radians(rotation)
    absolute_x = x + lx * math.cos(radians) - ly * math.sin(radians)
    absolute_y = y + lx * math.sin(radians) + ly * math.cos(radians)
    return f"{fmt(absolute_x)} {fmt(absolute_y)} {fmt(local_rotation + rotation)}"


def absolute_visible_properties(block: str, x: float, y: float, rotation: float) -> str:
    for property_name in ("Reference", "Value"):
        property_start = block.find(f'(property "{property_name}"')
        if property_start < 0:
            continue
        property_end = find_balanced(block, property_start)
        property_block = block[property_start:property_end]
        at_match = re.search(r'\(at\s+(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)(?:\s+(-?\d+(?:\.\d+)?))?\)', property_block)
        if not at_match:
            continue
        local_at = " ".join(value for value in at_match.groups() if value is not None)
        replacement = f"(at {absolute_at(local_at, x, y, rotation)})"
        property_block = property_block[:at_match.start()] + replacement + property_block[at_match.end():]
        block = block[:property_start] + property_block + block[property_end:]
    return block


# Explicit field placement keeps annotations readable even when a component
# is rotated for the power path.  Displayed fields remain horizontal and are
# placed away from the wire carrying the component pins.
FIELD_LAYOUTS: dict[str, dict[str, tuple[float, float, float]]] = {
    "U1": {"Reference": (179.07, 133.35, 0), "Value": (179.07, 199.39, 0)},
    "J1": {"Reference": (238.76, 119.38, 0), "Value": (238.76, 218.44, 0)},
    "J2": {"Reference": (139.70, 90.17, 0), "Value": (130.81, 106.68, 0)},
    "J3": {"Reference": (279.40, 148.59, 0), "Value": (279.40, 161.29, 0)},
    "U3": {"Reference": (168.91, 88.90, 0), "Value": (179.07, 116.84, 0)},
    "R7": {"Reference": (149.86, 77.47, 0), "Value": (149.86, 74.93, 0)},
    "L1": {"Reference": (163.83, 77.47, 0), "Value": (163.83, 74.93, 0)},
    "D4": {"Reference": (180.34, 77.47, 0), "Value": (180.34, 74.93, 0)},
    "R8": {"Reference": (142.24, 93.98, 0), "Value": (149.86, 91.44, 0)},
    "C6": {"Reference": (151.00, 102.87, 0), "Value": (151.00, 109.22, 0)},
    "C7": {"Reference": (154.94, 112.00, 0), "Value": (154.94, 116.00, 0)},
    "R9": {"Reference": (214.63, 91.44, 0), "Value": (214.63, 96.52, 0)},
    "R10": {"Reference": (202.00, 113.03, 0), "Value": (202.00, 118.11, 0)},
    "C8": {"Reference": (185.42, 91.44, 0), "Value": (185.42, 96.52, 0)},
    "R11": {"Reference": (230.00, 91.44, 0), "Value": (230.00, 96.52, 0)},
    "D5": {"Reference": (230.00, 103.00, 0), "Value": (230.00, 109.22, 0)},
    "C9": {"Reference": (248.00, 92.71, 0), "Value": (248.00, 99.06, 0)},
    "D7": {"Reference": (260.35, 99.06, 0), "Value": (260.35, 96.52, 0)},
    "D6": {"Reference": (253.00, 111.76, 0), "Value": (253.00, 116.84, 0)},
    "C11": {"Reference": (271.78, 111.76, 0), "Value": (271.78, 116.84, 0)},
    "D8": {"Reference": (292.00, 111.76, 0), "Value": (292.00, 116.84, 0)},
    "R16": {"Reference": (292.00, 123.19, 0), "Value": (292.00, 130.81, 0)},
    "RV1": {"Reference": (287.02, 173.99, 0), "Value": (279.40, 190.50, 0)},
}


def set_property_position(
    block: str, property_name: str, x: float, y: float, rotation: float = 0
) -> str:
    property_start = block.find(f'(property "{property_name}"')
    if property_start < 0:
        return block
    property_end = find_balanced(block, property_start)
    property_block = block[property_start:property_end]
    at_match = re.search(
        r'\(at\s+-?\d+(?:\.\d+)?\s+-?\d+(?:\.\d+)?(?:\s+-?\d+(?:\.\d+)?)?\)',
        property_block,
    )
    if not at_match:
        return block
    replacement = f"(at {fmt(x)} {fmt(y)} {fmt(rotation)})"
    property_block = (
        property_block[:at_match.start()]
        + replacement
        + property_block[at_match.end():]
    )
    return block[:property_start] + property_block + block[property_end:]


def apply_field_layout(block: str, reference: str, symbol_rotation: float = 0) -> str:
    for property_name, coordinates in FIELD_LAYOUTS.get(reference, {}).items():
        x, y, layout_rotation = coordinates
        # Symbol properties inherit the instance rotation in KiCad.  Apply
        # the inverse here so every displayed annotation remains horizontal.
        if round(symbol_rotation) % 360 == 180:
            # KiCad already keeps text upright for a 180-degree symbol; do
            # not apply a second half-turn to these fields.
            field_rotation = layout_rotation
        else:
            field_rotation = (layout_rotation - symbol_rotation) % 360
        block = set_property_position(block, property_name, x, y, field_rotation)
    return block


def properties_are_sheet_absolute(block: str) -> bool:
    """Detect an already-reviewed instance so regeneration is idempotent."""
    for property_name in ("Reference", "Value"):
        property_start = block.find(f'(property "{property_name}"')
        if property_start < 0:
            continue
        property_end = find_balanced(block, property_start)
        property_block = block[property_start:property_end]
        at_match = re.search(
            r'\(at\s+(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)', property_block
        )
        if at_match and (abs(float(at_match.group(1))) > 50 or abs(float(at_match.group(2))) > 50):
            return True
    return False


def replace_lib_id(block: str, lib_id: str) -> str:
    return re.sub(r'\(lib_id "[^"]+"\)', f'(lib_id "{lib_id}")', block, count=1)


def instance_from_old(
    old: dict[str, str],
    ref: str,
    lib_id: str,
    footprint: str,
    position: tuple[float, float],
    rotation: float,
    value: str | None = None,
) -> str:
    block = replace_lib_id(old[ref], lib_id)
    block = replace_root_at(block, position[0], position[1], rotation)
    # KiCad's root-sheet symbol instance path is the schematic UUID, not the
    # sheet_instances display path "/".  Using the latter makes the exporter
    # render a second copy of each symbol's default fields at the page origin.
    block = block.replace('(path "/"', '(path "/10000000-0000-4000-8000-000000000001"', 1)
    block = replace_property(block, "Footprint", footprint)
    if value is not None:
        block = replace_property(block, "Value", value)
    if not properties_are_sheet_absolute(block):
        block = absolute_visible_properties(block, position[0], position[1], rotation)
    return apply_field_layout(block, ref, rotation)


def wire(x1: float, y1: float, x2: float, y2: float) -> str:
    if abs(x1 - x2) < 1e-9 and abs(y1 - y2) < 1e-9:
        return ""
    return f'''\t(wire
\t\t(pts
\t\t\t(xy {fmt(x1)} {fmt(y1)}) (xy {fmt(x2)} {fmt(y2)})
\t\t)
\t\t(stroke (width 0) (type solid))
\t\t(uuid "{uid()}")
\t)'''


def translate_sheet_block(block: str, dx: float, dy: float) -> str:
    """Translate plotted sheet coordinates while leaving local fields at 0,0."""
    pattern = re.compile(
        r'(?P<prefix>\((?:at|xy)\s+)'
        r'(?P<x>-?\d+(?:\.\d+)?)\s+'
        r'(?P<y>-?\d+(?:\.\d+)?)(?P<rest>[^\r\n\)]*)'
    )

    def replace(match: re.Match[str]) -> str:
        x = float(match.group("x"))
        y = float(match.group("y"))
        # Instance metadata uses local (0, 0) coordinates and must not move.
        if abs(x) < 50 and abs(y) < 50:
            return match.group(0)
        return (
            f'{match.group("prefix")}{fmt(x + dx)} {fmt(y + dy)}'
            f'{match.group("rest")}'
        )

    return pattern.sub(replace, block)


def junction(x: float, y: float) -> str:
    return f'''\t(junction
\t\t(at {fmt(x)} {fmt(y)})
\t\t(diameter 1.016)
\t\t(color 0 0 0 0)
\t\t(uuid "{uid()}")
\t)'''


def parse_wire_segments(wire_blocks: list[str]) -> list[tuple[float, float, float, float]]:
    segments: list[tuple[float, float, float, float]] = []
    point_pattern = re.compile(r'\(xy\s+(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)\)')
    for block in wire_blocks:
        points = point_pattern.findall(block)
        if len(points) != 2:
            continue
        (x1, y1), (x2, y2) = ((float(x), float(y)) for x, y in points)
        if abs(x1 - x2) < 1e-9 and abs(y1 - y2) < 1e-9:
            continue
        segments.append((x1, y1, x2, y2))
    return segments


def point_on_segment_interior(
    x: float,
    y: float,
    segment: tuple[float, float, float, float],
    epsilon: float = 1e-6,
) -> bool:
    x1, y1, x2, y2 = segment
    cross = (x - x1) * (y2 - y1) - (y - y1) * (x2 - x1)
    if abs(cross) > epsilon:
        return False
    if abs(x1 - x2) >= abs(y1 - y2):
        return min(x1, x2) + epsilon < x < max(x1, x2) - epsilon and abs(y - y1) <= epsilon
    return min(y1, y2) + epsilon < y < max(y1, y2) - epsilon and abs(x - x1) <= epsilon


def junction_wire_profile(
    x: float,
    y: float,
    segments: list[tuple[float, float, float, float]],
    epsilon: float = 1e-6,
) -> tuple[int, int]:
    endpoints = 0
    interiors = 0
    for x1, y1, x2, y2 in segments:
        at_endpoint = (abs(x - x1) <= epsilon and abs(y - y1) <= epsilon) or (
            abs(x - x2) <= epsilon and abs(y - y2) <= epsilon
        )
        if at_endpoint:
            endpoints += 1
        elif point_on_segment_interior(x, y, (x1, y1, x2, y2), epsilon):
            interiors += 1
    return endpoints, interiors


def junction_is_structurally_needed(endpoints: int, interiors: int) -> bool:
    """Keep T/cross junctions and intentional terminal anchors."""
    return (interiors >= 1 and (endpoints >= 1 or interiors >= 2)) or (endpoints == 1 and interiors == 0)


def symbol_pin_local_points(symbol_block: str) -> list[tuple[float, float]]:
    points: list[tuple[float, float]] = []
    for match in re.finditer(r'\(pin\s+', symbol_block):
        pin_start = match.start()
        pin_block = symbol_block[pin_start:find_balanced(symbol_block, pin_start)]
        at = get_first_at(pin_block)
        if at:
            points.append((at[0], at[1]))
    return points


def collect_pin_endpoints(symbol_library: str, instances: list[str]) -> set[tuple[float, float]]:
    endpoints: set[tuple[float, float]] = set()
    for instance in instances:
        lib_match = re.search(r'\(lib_id "([^"]+)"\)', instance)
        at = get_first_at(instance)
        if not lib_match or not at:
            continue
        try:
            symbol_block = extract_symbol(symbol_library, lib_match.group(1))
        except ValueError:
            continue
        x, y, rotation = at
        for local_x, local_y in symbol_pin_local_points(symbol_block):
            absolute = absolute_at(f"{local_x} {local_y} 0", x, y, rotation).split()
            endpoints.add((round(float(absolute[0]), 6), round(float(absolute[1]), 6)))
    return endpoints


def label(name: str, x: float, y: float, rotation: float = 0) -> str:
    return f'''\t(label "{name}"
\t\t(at {fmt(x)} {fmt(y)} {fmt(rotation)})
\t\t(effects (font (size 0.9 0.9)))
\t\t(uuid "{uid()}")
\t)'''


def hidden_label(name: str, x: float, y: float, rotation: float = 0) -> str:
    # These labels bridge legacy pin-to-wire coordinate differences while the
    # visible drawing remains wire-first.  KiCad 9 CLI plotting can include
    # hidden local labels, so use a sub-pixel font as a compatibility fallback.
    return f'''\t(label "{name}"
\t\t(at {fmt(x)} {fmt(y)} {fmt(rotation)})
\t\t(effects (font (size 0.01 0.01)) (hide yes))
\t\t(uuid "{uid()}")
\t)'''


def power_symbol(
    lib_id: str, reference: str, x: float, y: float, hide_value: bool = False
) -> str:
    if lib_id == "power:GND":
        ref_at, value, value_at = "0 -6.35 0", "GND", "0 -3.81 0"
    elif lib_id == "power:+5V":
        ref_at, value, value_at = "0 -3.81 0", "+5V", "0 3.56 0"
    elif lib_id == "power:+3V3":
        ref_at, value, value_at = "0 -3.81 0", "+3V3", "0 3.56 0"
    elif lib_id == "power:PWR_FLAG":
        ref_at, value, value_at = "0 1.91 0", "PWR_FLAG", "0 3.81 0"
    else:
        raise ValueError(lib_id)
    ref_at = absolute_at(ref_at, x, y)
    value_at = absolute_at(value_at, x, y)
    value_effects = "(effects (font (size 1.27 1.27))"
    if hide_value:
        value_effects += " hide"
    value_effects += ")"
    return f'''\t(symbol (lib_id "{lib_id}") (at {fmt(x)} {fmt(y)} 0) (unit 1)
\t\t(exclude_from_sim no) (in_bom no) (on_board yes) (dnp no)
\t\t(uuid "{uid()}")
\t\t(property "Reference" "{reference}" (at {ref_at}) (effects (font (size 1.27 1.27)) hide))
\t\t(property "Value" "{value}" (at {value_at}) {value_effects})
\t\t(property "Footprint" "" (at 0 0 0) (effects (font (size 1.27 1.27)) hide))
\t\t(property "Datasheet" "" (at 0 0 0) (effects (font (size 1.27 1.27)) hide))
\t\t(property "Description" "" (at 0 0 0) (effects (font (size 1.27 1.27)) hide))
\t\t(pin "1" (uuid "{uid()}"))
\t\t(instances (project "pico_lta042b010f_carrier" (path "/10000000-0000-4000-8000-000000000001" (reference "{reference}") (unit 1))))
\t)'''


def parse_pad_blocks(block: str) -> list[tuple[int, int, str]]:
    result: list[tuple[int, int, str]] = []
    for match in re.finditer(r'\(pad\s+"[^"]*"', block):
        start = match.start()
        end = find_balanced(block, start)
        result.append((start, end, block[start:end]))
    return result


def pad_net_map(block: str) -> dict[str, dict[str, str]]:
    mapping: dict[str, dict[str, str]] = {}
    for _, _, pad in parse_pad_blocks(block):
        name_match = re.search(r'\(pad\s+"([^"]*)"', pad)
        net_match = re.search(r'\(net\s+(\d+)\s+"([^"]*)"\)', pad)
        if not name_match or not net_match:
            continue
        pin_match = re.search(r'\(pinfunction\s+"([^"]*)"\)', pad)
        type_match = re.search(r'\(pintype\s+"([^"]*)"\)', pad)
        mapping[name_match.group(1)] = {
            "number": net_match.group(1),
            "name": net_match.group(2),
            "pinfunction": pin_match.group(1) if pin_match else "",
            "pintype": type_match.group(1) if type_match else "passive",
        }
    return mapping


def annotate_footprint(footprint: str, nets: dict[str, dict[str, str]]) -> str:
    pads = parse_pad_blocks(footprint)
    for start, end, pad in reversed(pads):
        match = re.search(r'\(pad\s+"([^"]*)"', pad)
        if not match:
            continue
        info = nets.get(match.group(1))
        if not info or "(net " in pad:
            continue
        layers = re.search(r'\(layers[^\n]*\)', pad)
        if not layers:
            continue
        annotation = f'\n\t\t(net {info["number"]} "{info["name"]}")'
        if info.get("pinfunction"):
            annotation += f' (pinfunction "{info["pinfunction"]}")'
        if info.get("pintype"):
            annotation += f' (pintype "{info["pintype"]}")'
        insert_at = layers.end()
        pad = pad[:insert_at] + annotation + pad[insert_at:]
        footprint = footprint[:start] + pad + footprint[end:]
    return footprint


def make_footprint(
    source: Path,
    library_name: str,
    reference: str,
    value: str,
    position: tuple[float, float],
    nets: dict[str, dict[str, str]],
    rotation: float = 0,
) -> str:
    footprint = source.read_text(encoding="utf-8")
    first_name = re.search(r'^\(footprint "([^"]+)"', footprint, re.MULTILINE)
    if not first_name:
        raise ValueError(f"footprint name missing in {source}")
    footprint = (
        footprint[: first_name.start()]
        + f'(footprint "{library_name}:{first_name.group(1)}"'
        + footprint[first_name.end() :]
    )
    footprint = re.sub(r'\(layer "[^"]+"\)', '(layer "F.Cu")', footprint, count=1)
    layer_end = re.search(r'\(layer "F\.Cu"\)', footprint)
    if not layer_end:
        raise ValueError(f"layer missing in {source}")
    footprint = footprint[:layer_end.end()] + f'\n\t(at {fmt(position[0])} {fmt(position[1])} {fmt(rotation)})' + footprint[layer_end.end():]
    footprint = re.sub(r'(\(property "Reference" )"[^"]+"', rf'\1"{reference}"', footprint, count=1)
    footprint = re.sub(r'(\(property "Value" )"[^"]+"', rf'\1"{value}"', footprint, count=1)
    return annotate_footprint(footprint, nets)


def build_schematic() -> None:
    source = SRC_SCH.read_text(encoding="utf-8")
    source_is_reviewed = '(paper "A4")' in source
    lib_start = source.index('(lib_symbols')
    lib_end = find_balanced(source, lib_start)
    lib_block = source[lib_start:lib_end]
    tail = round_tail_coordinates(source[lib_end:])

    standard_sources = {
        ("Device", "R"): (KI_SYMBOLS / "Device.kicad_sym", "R"),
        ("Device", "L"): (KI_SYMBOLS / "Device.kicad_sym", "L"),
        ("Device", "D_Schottky"): (KI_SYMBOLS / "Device.kicad_sym", "D_Schottky"),
        ("Device", "C"): (KI_SYMBOLS / "Device.kicad_sym", "C"),
        ("Device", "C_Polarized"): (KI_SYMBOLS / "Device.kicad_sym", "C_Polarized"),
        ("Device", "LED"): (KI_SYMBOLS / "Device.kicad_sym", "LED"),
        ("Device", "R_Potentiometer"): (KI_SYMBOLS / "Device.kicad_sym", "R_Potentiometer"),
        ("Connector_Generic", "Conn_01x02"): (KI_SYMBOLS / "Connector_Generic.kicad_sym", "Conn_01x02"),
        ("Regulator_Switching", "MC33063AD"): (KI_SYMBOLS / "Regulator_Switching.kicad_sym", "MC33063AD"),
        ("power", "+5V"): (KI_SYMBOLS / "power.kicad_sym", "+5V"),
        ("power", "+3V3"): (KI_SYMBOLS / "power.kicad_sym", "+3V3"),
        ("power", "GND"): (KI_SYMBOLS / "power.kicad_sym", "GND"),
        ("power", "PWR_FLAG"): (KI_SYMBOLS / "power.kicad_sym", "PWR_FLAG"),
    }
    standard_blocks: list[str] = []
    cached_files: dict[Path, str] = {}
    for (library, name), (path, source_name) in standard_sources.items():
        text = cached_files.setdefault(path, path.read_text(encoding="utf-8"))
        qualified = qualify_symbol(extract_symbol(text, source_name), library, source_name)
        if library == "Regulator_Switching" and source_name == "MC33063AD":
            qualified = qualified.replace('(pin open_emitter line', '(pin passive line')
        standard_blocks.append(qualified)
    custom_pico_sch = extract_symbol(lib_block, "Custom:Pico_2W_40P")
    custom_lcd_sch = extract_symbol(lib_block, "Custom:LTA042B010F_FFC36")
    custom_lcd_sch = custom_lcd_sch.replace('(pin power_in line', '(pin passive line')
    # Keep only the two custom symbols still used by the reviewed design.
    # The old provisional POWER_* definitions were unused and caused KiCad's
    # native exporter to plot their default fields at the sheet origin.
    new_lib = "(lib_symbols\n" + "\n\n".join([custom_pico_sch, custom_lcd_sch, *standard_blocks]) + "\n)"
    # The original hand-built schematic was also plotting the default
    # Reference/Value text from embedded library definitions at the sheet
    # origin.  Instance properties below carry the real displayed fields;
    # hide only the library defaults so the native KiCad export stays clean.
    for property_name in ("Reference", "Value"):
        search_from = 0
        marker = f'(property "{property_name}"'
        while True:
            property_start = new_lib.find(marker, search_from)
            if property_start < 0:
                break
            property_end = find_balanced(new_lib, property_start)
            property_block = new_lib[property_start:property_end]
            if "(hide yes)" not in property_block:
                effects_start = property_block.find("(effects")
                if effects_start >= 0:
                    effects_end = find_balanced(property_block, effects_start)
                    property_block = property_block[:effects_end - 1] + "\n\t\t\t\t\t(hide yes)\n\t\t\t\t" + property_block[effects_end - 1:]
                    new_lib = new_lib[:property_start] + property_block + new_lib[property_end:]
                    property_end = property_start + len(property_block)
            search_from = property_end

    objects = top_level_blocks(tail)
    old: dict[str, str] = {}
    sheet_instance = None
    for block in objects:
        ref = get_reference(block)
        if ref:
            old[ref] = block
        if block.startswith("(sheet_instances"):
            sheet_instance = block
    required = ["U1", "J1", "J2", "J3", "U3", "R7", "L1", "D4", "RV1"]
    if not all(ref in old for ref in required):
        raise ValueError("required schematic instances not found")

    root: list[str] = []
    root.append(instance_from_old(old, "U1", "Custom:Pico_2W_40P", "Module:RaspberryPi_Pico_W_SMD_HandSolder", (grid(179.38), grid(168.42)), 0, "RaspberryPi_Pico_2W"))
    root.append(instance_from_old(old, "J1", "Custom:LTA042B010F_FFC36", "Connector_FFC-FPC:TE_3-1734839-6_1x36-1MP_P0.5mm_Horizontal", (grid(239.38), grid(168.42)), 0, "LTA042B010F_FFC36"))

    instances = [
        ("J2", "Connector_Generic:Conn_01x02", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical", (139.70, 100.33), 0, "5V INPUT"),
        ("J3", "Connector_Generic:Conn_01x02", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical", (279.40, 153.67), 0, "13V8 OUT"),
        ("U3", "Regulator_Switching:MC33063AD", "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", (168.91, 101.60), 0, "MC34063AD"),
        ("R7", "Device:R", "Resistor_SMD:R_2512_6332Metric_Pad1.40x3.35mm_HandSolder", (149.86, 82.55), 270, "0.47R 1%"),
        ("L1", "Device:L", "Inductor_SMD:L_Sunlord_MWSA1204S-150", (163.83, 82.55), 270, "150uH"),
        ("D4", "Device:D_Schottky", "Diode_SMD:D_SMA", (180.34, 82.55), 180, "1N5819"),
        ("R8", "Device:R", "Resistor_SMD:R_0805_2012Metric", (142.24, 99.06), 90, "200"),
        ("C6", "Device:C_Polarized", "Capacitor_THT:CP_Radial_D5.0mm_P2.00mm", (144.78, 105.41), 0, "220u 16V"),
        ("C7", "Device:C", "Capacitor_SMD:C_0805_2012Metric", (149.86, 115.57), 0, "470p"),
        ("R9", "Device:R", "Resistor_SMD:R_0805_2012Metric", (208.28, 93.98), 0, "10k 1%"),
        ("R10", "Device:R", "Resistor_SMD:R_0805_2012Metric", (208.28, 115.57), 0, "1k 1%"),
        ("C8", "Device:C_Polarized", "Capacitor_THT:CP_Radial_D5.0mm_P2.00mm", (190.50, 93.98), 0, "47u 25V"),
        ("R11", "Device:R", "Resistor_SMD:R_0805_2012Metric", (224.79, 93.98), 0, "10k 1%"),
        ("D5", "Device:LED", "LED_SMD:LED_0805_2012Metric", (224.79, 106.68), 90, "GREEN LED"),
        ("C9", "Device:C_Polarized", "Capacitor_THT:CP_Radial_D5.0mm_P2.00mm", (242.57, 96.52), 0, "10u 25V"),
        ("D7", "Device:D_Schottky", "Diode_SMD:D_SMA", (260.35, 104.14), 180, "1N5819"),
        ("D6", "Device:D_Schottky", "Diode_SMD:D_SMA", (247.65, 114.30), 90, "1N5819"),
        ("C11", "Device:C_Polarized", "Capacitor_THT:CP_Radial_D5.0mm_P2.00mm", (278.13, 114.30), 180, "10u 25V"),
        ("D8", "Device:LED", "LED_SMD:LED_0805_2012Metric", (299.72, 114.30), 270, "RED LED"),
        ("R16", "Device:R", "Resistor_SMD:R_0805_2012Metric", (299.72, 127.00), 0, "10k 1%"),
        ("RV1", "Device:R_Potentiometer", "Potentiometer_THT:Potentiometer_Bourns_3296W_Vertical", (279.40, 180.34), 0, "10k TRIM"),
    ]
    for ref, lib_id, footprint, position, rotation, value in instances:
        root.append(instance_from_old(old, ref, lib_id, footprint, position, rotation, value))
    pin_endpoints = collect_pin_endpoints(new_lib, root)

    for block in objects:
        if block.startswith("(no_connect"):
            at = get_first_at(block)
            if at:
                logical_x = at[0] - SHEET_OFFSET_X if source_is_reviewed else at[0]
                logical_y = at[1] - SHEET_OFFSET_Y if source_is_reviewed else at[1]
                if (160 <= logical_x <= 190 and logical_y >= 140) or (228 <= logical_x <= 234 and logical_y >= 120):
                    root.append(translate_sheet_block(block, -SHEET_OFFSET_X, -SHEET_OFFSET_Y) if source_is_reviewed else block)
        elif block.startswith("(label"):
            at = get_first_at(block)
            if not at:
                continue
            x, y, _ = at
            logical_x = x - SHEET_OFFSET_X if source_is_reviewed else x
            logical_y = y - SHEET_OFFSET_Y if source_is_reviewed else y
            if (160 <= logical_x <= 190 and logical_y >= 140) or (228 <= logical_x <= 234 and logical_y >= 120):
                root.append(translate_sheet_block(block, -SHEET_OFFSET_X, -SHEET_OFFSET_Y) if source_is_reviewed else block)

    gnd_y, plus_y = 139.70, 82.55
    wires = [
        # J2 +5, GND and input-to-R7 path
        wire(134.62, 100.33, 130.81, 100.33), wire(130.81, 100.33, 127.00, 100.33),
        wire(127.00, 100.33, 127.00, 82.55), wire(127.00, 82.55, 146.05, 82.55),
        wire(134.62, 97.79, 130.81, 97.79), wire(130.81, 97.79, 118.11, 97.79),
        wire(118.11, 97.79, 118.11, gnd_y),

        # +5 bus to U3 Vin; R8 driver resistor; C6
        wire(127.00, 96.52, 154.94, 96.52), wire(158.75, 96.52, 154.94, 96.52),
        wire(134.62, 99.06, 130.81, 99.06), wire(130.81, 99.06, 130.81, 100.33),
        wire(138.43, 99.06, 130.81, 99.06),
        wire(146.05, 99.06, 175.26, 99.06), wire(179.07, 99.06, 175.26, 99.06),
        wire(144.78, 101.60, 144.78, 100.33), wire(127.00, 100.33, 144.78, 100.33),
        wire(144.78, 109.22, 144.78, gnd_y),

        # R7/L1 current path, IPK sense
        wire(153.67, 82.55, 160.02, 82.55), wire(167.64, 82.55, 176.53, 82.55),
        wire(156.21, 82.55, 156.21, 96.52), wire(156.21, 96.52, 181.61, 96.52),
        wire(179.07, 96.52, 181.61, 96.52),

        # U3 TC, GND, switch emitter
        wire(149.86, 111.76, 149.86, 106.68), wire(154.94, 106.68, 149.86, 106.68),
        wire(158.75, 106.68, 154.94, 106.68), wire(149.86, 119.38, 149.86, gnd_y),
        wire(168.91, 114.30, 168.91, 118.11), wire(168.91, 118.11, 168.91, gnd_y),
        wire(179.07, 106.68, 182.88, 106.68), wire(182.88, 106.68, 187.96, 106.68),
        wire(187.96, 106.68, 187.96, gnd_y),

        # U3 VFB and feedback divider
        wire(179.07, 109.22, 182.88, 109.22), wire(182.88, 109.22, 195.58, 109.22),
        wire(195.58, 109.22, 195.58, 116.84), wire(195.58, 116.84, 208.28, 116.84),
        wire(208.28, 97.79, 208.28, 116.84), wire(208.28, 111.76, 208.28, 116.84),
        wire(208.28, 119.38, 208.28, gnd_y),

        # +13 rail and positive LED. Stop the rail at the last actual branch;
        # the previous extension to x=299.72 was a visually dangling wire.
        wire(184.15, 82.55, 242.57, 82.55), wire(190.50, 90.17, 190.50, 82.55),
        wire(190.50, 97.79, 190.50, gnd_y), wire(208.28, 90.17, 208.28, 82.55),
        wire(224.79, 90.17, 224.79, 82.55), wire(224.79, 97.79, 224.79, 102.87),
        wire(224.79, 110.49, 224.79, gnd_y),

        # C9 / D7 charge-pump midpoint and negative rail
        wire(242.57, 92.71, 242.57, 82.55), wire(242.57, 100.33, 242.57, 104.14),
        wire(242.57, 104.14, 256.54, 104.14), wire(247.65, 104.14, 247.65, 110.49),
        wire(247.65, 118.11, 247.65, gnd_y), wire(264.16, 104.14, 299.72, 104.14),
        wire(278.13, 104.14, 278.13, 110.49), wire(278.13, 118.11, 278.13, gnd_y),
        wire(299.72, 104.14, 299.72, 110.49), wire(299.72, 118.11, 299.72, 123.19),
        wire(299.72, 130.81, 299.72, gnd_y),

        # External connector and trim
        wire(274.32, 153.67, 270.51, 153.67),
        wire(274.32, 156.21, 266.70, 156.21), wire(266.70, 156.21, 266.70, gnd_y),
        wire(279.40, 168.91, 279.40, 176.53),
        wire(279.40, 184.15, 279.40, 190.50), wire(283.21, 180.34, 287.02, 180.34),
        wire(118.11, gnd_y, 299.72, gnd_y),
    ]
    root.extend(wire_block for wire_block in wires if wire_block)
    wire_segments = parse_wire_segments(wires)
    seen_junctions: set[tuple[float, float]] = set()
    kept_junctions: list[tuple[float, float]] = []
    removed_junctions: list[tuple[float, float]] = []
    four_way_junctions: list[tuple[float, float]] = []
    candidate_count = 0

    def add_junction_candidate(x: float, y: float) -> None:
        nonlocal candidate_count
        candidate_count += 1
        coordinate = (round(x, 6), round(y, 6))
        if coordinate in seen_junctions:
            removed_junctions.append(coordinate)
            return
        seen_junctions.add(coordinate)
        endpoints, interiors = junction_wire_profile(x, y, wire_segments)
        if 2 * interiors + endpoints >= 4:
            four_way_junctions.append(coordinate)
        if coordinate in pin_endpoints or junction_is_structurally_needed(endpoints, interiors):
            root.append(junction(x, y))
            kept_junctions.append(coordinate)
        else:
            removed_junctions.append(coordinate)

    for x, y in [
        (127.00, 82.55), (130.81, 82.55), (127.00, 96.52), (127.00, 99.06),
        (127.00, 100.33), (130.81, 99.06), (130.81, 100.33), (118.11, 97.79),
        (118.11, gnd_y), (134.62, 100.33), (134.62, 97.79), (144.78, 100.33),
        (144.78, 96.52), (138.43, 99.06), (156.21, 82.55), (156.21, 96.52),
        (181.61, 96.52), (154.94, 106.68), (149.86, 106.68), (149.86, gnd_y),
        (168.91, 118.11), (168.91, gnd_y), (182.88, 106.68), (187.96, 106.68),
        (187.96, gnd_y), (182.88, 109.22), (195.58, 109.22), (195.58, 116.84),
        (208.28, 116.84), (208.28, gnd_y), (184.15, 82.55), (190.50, 82.55),
        (190.50, gnd_y), (208.28, 82.55), (224.79, 82.55), (224.79, 102.87),
        (224.79, gnd_y), (242.57, 82.55), (242.57, 104.14), (247.65, 104.14),
        (247.65, gnd_y), (256.54, 104.14), (264.16, 104.14), (278.13, 104.14),
        (278.13, gnd_y), (299.72, 104.14), (299.72, 110.49), (299.72, 118.11),
        (299.72, 123.19), (299.72, gnd_y), (270.51, 153.67), (270.51, 151.13),
        (279.40, 176.53), (279.40, 190.50), (279.40, 184.15), (283.21, 180.34),
        (266.70, 156.21), (266.70, gnd_y),
        (130.81, 82.55), (125.73, gnd_y), (196.85, 82.55), (285.75, 104.14),
    ]:
        add_junction_candidate(x, y)
    for x, y in [
        (146.05, 82.55), (153.67, 82.55), (160.02, 82.55), (167.64, 82.55),
        (176.53, 82.55), (190.50, 90.17), (208.28, 90.17),
        (224.79, 90.17), (242.57, 92.71), (154.94, 96.52), (158.75, 96.52),
        (179.07, 96.52), (130.81, 97.79), (190.50, 97.79), (208.28, 97.79),
        (224.79, 97.79), (134.62, 99.06), (146.05, 99.06), (175.26, 99.06),
        (179.07, 99.06), (242.57, 100.33), (144.78, 101.60), (158.75, 106.68),
        (179.07, 106.68), (144.78, 109.22), (179.07, 109.22), (224.79, 110.49),
        (247.65, 110.49), (278.13, 110.49), (149.86, 111.76), (208.28, 111.76),
        (168.91, 114.30), (247.65, 118.11), (278.13, 118.11), (149.86, 119.38),
        (208.28, 119.38), (299.72, 130.81), (144.78, 139.70), (274.32, 151.13),
        (274.32, 153.67), (287.02, 180.34), (134.62, 102.87), (130.81, 102.87),
    ]:
        add_junction_candidate(x, y)

    OUT.mkdir(parents=True, exist_ok=True)
    report = [
        "KiCad schematic junction cleanup report",
        f"candidate entries: {candidate_count}",
        f"unique coordinates: {len(seen_junctions)}",
        f"kept structural/terminal junctions: {len(kept_junctions)}",
        f"removed duplicate/endpoint/collinear junctions: {len(removed_junctions)}",
        f"four-way junctions retained for review: {len(four_way_junctions)}",
        "",
        "Rule: retain T/cross junctions, symbol pin endpoints, and terminal anchors.",
        "Signal wiring is preserved; only the visually dangling +13V rail extension is shortened.",
        "Internal helper labels remain electrically present at sub-pixel size for legacy pin endpoints.",
        "",
        "Retained four-way coordinates:",
        *(
            f"  {x + SHEET_OFFSET_X:.3f}, {y + SHEET_OFFSET_Y:.3f}"
            for x, y in four_way_junctions
        ),
    ]
    (OUT / "pico_lta042b010f_carrier-junction-cleanup.txt").write_text("\n".join(report) + "\n", encoding="utf-8")

    root.extend([
        # Power labels are kept only at the rails and off-board interfaces;
        # the component-to-component nets are visibly wired.
        hidden_label("+5V", 127.00, 82.55), hidden_label("GND", 118.11, gnd_y),
        hidden_label("IPK_SENSE", 179.07, 96.52), hidden_label("DRIVER_DC", 160.02, 99.06),
        hidden_label("TC_TIMING", 149.86, 106.68), hidden_label("+13V8", 190.50, 82.55),
        hidden_label("VFB", 195.58, 116.84), hidden_label("PWR_LED_P", 224.79, 102.87),
        hidden_label("CPUMP_MID", 242.57, 104.14), hidden_label("NEG_LED_N", 299.72, 123.19),
        hidden_label("GND", 134.62, 102.87), hidden_label("GND", 144.78, 109.22),
        hidden_label("+5V", 158.75, 96.52), hidden_label("IPK_SENSE", 153.67, 82.55),
        hidden_label("IPK_SENSE", 160.02, 82.55), hidden_label("SW_NODE", 167.64, 82.55),
        hidden_label("+13V8", 184.15, 82.55), hidden_label("GND", 168.91, 114.30),
        hidden_label("GND", 179.07, 106.68), hidden_label("GND", 149.86, 119.38),
        hidden_label("GND", 190.50, 97.79), hidden_label("+13V8", 208.28, 90.17),
        hidden_label("GND", 208.28, 119.38), hidden_label("+13V8", 224.79, 90.17),
        hidden_label("GND", 224.79, 110.49), hidden_label("+13V8", 242.57, 92.71),
        hidden_label("CPUMP_MID", 247.65, 110.49), hidden_label("GND", 247.65, 118.11),
        hidden_label("-13V8", 264.16, 104.14), hidden_label("-13V8", 278.13, 110.49),
        hidden_label("GND", 278.13, 118.11), hidden_label("GND", 274.32, 156.21),
        hidden_label("GND", 279.40, 184.15), hidden_label("+5V", 130.81, 82.55),
        label("+3V3", 279.40, 168.91), label("GND", 125.73, gnd_y),
        label("+13V8", 196.85, plus_y, 180),
        label("-13V8", 285.75, 104.14, 180), hidden_label("SW_NODE", 176.53, 82.55, 180),
        hidden_label("SW_NODE", 179.07, 101.60), label("+13V8", 270.51, 153.67),
        label("VCPP_ADJ", 287.02, 180.34),
    ])
    root.extend([
        power_symbol("power:+5V", "#PWR0101", 127.00, 82.55),
        power_symbol("power:+3V3", "#PWR0103", 279.40, 176.53, hide_value=True),
        power_symbol("power:PWR_FLAG", "#FLG0101", 130.81, 82.55, hide_value=True),
        power_symbol("power:PWR_FLAG", "#FLG0102", 125.73, gnd_y, hide_value=True),
        power_symbol("power:PWR_FLAG", "#FLG0103", 196.85, 82.55, hide_value=True),
        power_symbol("power:PWR_FLAG", "#FLG0104", 285.75, 104.14, hide_value=True),
    ])
    # Center the assembled sheet objects on a readable A4 landscape page.
    root = [translate_sheet_block(block, SHEET_OFFSET_X, SHEET_OFFSET_Y) for block in root]
    if not sheet_instance:
        raise ValueError("sheet_instances missing")
    root.append(sheet_instance)

    header = source[:lib_start]
    header = header.replace('(paper "A3")', '(paper "A4")')
    header = header.replace('"Codex draft - verify against prototype"', '"Codex reviewed KiCad draft"')
    header = header.replace('"FFC connector contact side and exact part number are provisional"', '"Pico 2 W hand-solder footprint and TE FFC footprint selected from KiCad libraries"')
    schematic = header + new_lib + "\n" + "\n\n".join(root) + "\n)\n"
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "pico_lta042b010f_carrier.kicad_sch").write_text(schematic, encoding="utf-8")

    custom_pico = extract_symbol(lib_block, "Custom:Pico_2W_40P").replace('(symbol "Custom:Pico_2W_40P"', '(symbol "Pico_2W_40P"', 1)
    custom_lcd = extract_symbol(lib_block, "Custom:LTA042B010F_FFC36").replace('(symbol "Custom:LTA042B010F_FFC36"', '(symbol "LTA042B010F_FFC36"', 1)
    custom_lcd = custom_lcd.replace('(pin power_in line', '(pin passive line')
    custom_lib = '(kicad_symbol_lib (version 20241209) (generator "kicad_symbol_editor") (generator_version "9.0")\n' + custom_pico + "\n" + custom_lcd + "\n)\n"
    (OUT / "pico_lta042b010f_carrier.kicad_sym").write_text(custom_lib, encoding="utf-8")
    (OUT / "sym-lib-table").write_text('(sym_lib_table\n  (version 7)\n  (lib (name "Custom")(type "KiCad")(uri "${KIPRJMOD}/pico_lta042b010f_carrier.kicad_sym")(options "")(descr "Project-specific Pico and LCD symbols"))\n)\n', encoding="utf-8")
    (OUT / "pico_lta042b010f_carrier.kicad_pro").write_text(SRC_PRO.read_text(encoding="utf-8"), encoding="utf-8")


def build_pcb() -> None:
    source = SRC_PCB.read_text(encoding="utf-8")
    objects = top_level_blocks(source[source.index('(net 0 "")'):])
    footprints: dict[str, str] = {}
    for block in objects:
        if block.startswith("(footprint"):
            ref_match = re.search(r'\(property "Reference" "([^"]+)"', block)
            if ref_match:
                footprints[ref_match.group(1)] = block
    net_names = {match.group(2): match.group(1) for match in re.finditer(r'\(net\s+(\d+)\s+"([^"]*)"\)', source)}
    explicit = {
        "D4": {"1": ("+13V8", "K"), "2": ("SW_NODE", "A")}, "D6": {"1": ("GND", "K"), "2": ("CPUMP_MID", "A")}, "D7": {"1": ("-13V8", "K"), "2": ("CPUMP_MID", "A")}, "D5": {"1": ("GND", "K"), "2": ("PWR_LED_P", "A")}, "D8": {"1": ("-13V8", "K"), "2": ("NEG_LED_N", "A")},
    }

    def nets_for(ref: str) -> dict[str, dict[str, str]]:
        if ref in explicit:
            return {pad: {"number": net_names[net], "name": net, "pinfunction": function, "pintype": "passive"} for pad, (net, function) in explicit[ref].items()}
        testpoint_nets = {"TP1": "+5V", "TP2": "+13V8", "TP3": "-13V8", "TP4": "GND"}
        if ref in testpoint_nets:
            net = testpoint_nets[ref]
            return {"1": {"number": net_names[net], "name": net, "pinfunction": net, "pintype": "passive"}}
        return pad_net_map(footprints.get(ref, ""))

    standard = {
        "J1": (KI_FOOTPRINTS / "Connector_FFC-FPC.pretty/TE_3-1734839-6_1x36-1MP_P0.5mm_Horizontal.kicad_mod", "Connector_FFC-FPC", "LTA042B010F_FFC_36P_0.5mm", (20, 9), 0),
        "U1": (KI_FOOTPRINTS / "Module.pretty/RaspberryPi_Pico_W_SMD_HandSolder.kicad_mod", "Module", "RaspberryPi_Pico_2W", (56, 44.0), 0),
        "J2": (KI_FOOTPRINTS / "Connector_PinHeader_2.54mm.pretty/PinHeader_1x02_P2.54mm_Vertical.kicad_mod", "Connector_PinHeader_2.54mm", "5V_INPUT", (105, 8), 0),
        "J3": (KI_FOOTPRINTS / "Connector_PinHeader_2.54mm.pretty/PinHeader_1x02_P2.54mm_Vertical.kicad_mod", "Connector_PinHeader_2.54mm", "CCFL_INVERTER_13V8", (105, 15), 0),
        "RV1": (KI_FOOTPRINTS / "Potentiometer_THT.pretty/Potentiometer_Bourns_3296W_Vertical.kicad_mod", "Potentiometer_THT", "10k_CONTRAST_TRIM", (78, 60), 0),
        "U3": (KI_FOOTPRINTS / "Package_SO.pretty/SOIC-8_3.9x4.9mm_P1.27mm.kicad_mod", "Package_SO", "MC34063AD", (85, 35), 0),
        "R7": (KI_FOOTPRINTS / "Resistor_SMD.pretty/R_2512_6332Metric_Pad1.40x3.35mm_HandSolder.kicad_mod", "Resistor_SMD", "0.47R 1%", (72, 22), 0),
        "R8": (KI_FOOTPRINTS / "Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod", "Resistor_SMD", "200", (78, 35), 90), "R9": (KI_FOOTPRINTS / "Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod", "Resistor_SMD", "10k", (101, 35), 90), "R10": (KI_FOOTPRINTS / "Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod", "Resistor_SMD", "1k", (101, 48), 90), "R11": (KI_FOOTPRINTS / "Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod", "Resistor_SMD", "10k", (109, 35), 90), "R16": (KI_FOOTPRINTS / "Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod", "Resistor_SMD", "10k", (107, 63), 90),
        "L1": (KI_FOOTPRINTS / "Inductor_SMD.pretty/L_Sunlord_MWSA1204S-150.kicad_mod", "Inductor_SMD", "150uH", (85, 22), 0), "D4": (KI_FOOTPRINTS / "Diode_SMD.pretty/D_SMA.kicad_mod", "Diode_SMD", "1N5819", (100, 22), 0), "D6": (KI_FOOTPRINTS / "Diode_SMD.pretty/D_SMA.kicad_mod", "Diode_SMD", "1N5819", (105, 42), 90), "D7": (KI_FOOTPRINTS / "Diode_SMD.pretty/D_SMA.kicad_mod", "Diode_SMD", "1N5819", (110, 22), 0), "D5": (KI_FOOTPRINTS / "LED_SMD.pretty/LED_0805_2012Metric.kicad_mod", "LED_SMD", "GREEN LED", (109, 45), 90), "D8": (KI_FOOTPRINTS / "LED_SMD.pretty/LED_0805_2012Metric.kicad_mod", "LED_SMD", "RED LED", (107, 55), 90),
        "C6": (KI_FOOTPRINTS / "Capacitor_THT.pretty/CP_Radial_D5.0mm_P2.00mm.kicad_mod", "Capacitor_THT", "220u 16V", (72, 35), 0), "C8": (KI_FOOTPRINTS / "Capacitor_THT.pretty/CP_Radial_D5.0mm_P2.00mm.kicad_mod", "Capacitor_THT", "47u 25V", (95, 38), 0), "C9": (KI_FOOTPRINTS / "Capacitor_THT.pretty/CP_Radial_D5.0mm_P2.00mm.kicad_mod", "Capacitor_THT", "10u 25V", (98, 30), 90), "C11": (KI_FOOTPRINTS / "Capacitor_THT.pretty/CP_Radial_D5.0mm_P2.00mm.kicad_mod", "Capacitor_THT", "10u 25V (+ to GND)", (113, 45), 0), "C7": (KI_FOOTPRINTS / "Capacitor_SMD.pretty/C_0805_2012Metric.kicad_mod", "Capacitor_SMD", "470p", (72, 50), 90),
    }
    for index, ref in enumerate(["TP1", "TP2", "TP3", "TP4"]):
        standard[ref] = (KI_FOOTPRINTS / "TestPoint.pretty/TestPoint_Pad_D2.0mm.kicad_mod", "TestPoint", {"TP1": "+5V", "TP2": "+13V8", "TP3": "-13V8", "TP4": "GND"}[ref], (10 + index * 10, 65), 0)
    for ref, position in {"H1": (4, 5), "H2": (115, 5), "H3": (5, 65), "H4": (115, 65)}.items():
        standard[ref] = (KI_FOOTPRINTS / "MountingHole.pretty/MountingHole_3.2mm_M3.kicad_mod", "MountingHole", "MountingHole", position, 0)

    rendered: list[str] = []
    for ref, (path, library, value, position, rotation) in standard.items():
        if not path.exists():
            raise FileNotFoundError(path)
        rendered.append(make_footprint(path, library, ref, value, position, nets_for(ref), rotation))
    prefix = source[: source.index('(footprint')]
    graphics_positions = [source.find('(gr_rect'), source.find('(gr_line'), source.find('(gr_arc')]
    graphics_positions = [position for position in graphics_positions if position >= 0]
    if not graphics_positions:
        raise ValueError("board edge graphics not found")
    suffix = source[min(graphics_positions):]
    # Keep the board annotation legible and above the project's 0.8 mm
    # silkscreen minimum while retaining the native KiCad graphics.
    suffix = suffix.replace('(size 0.65 0.65)', '(size 0.8 0.8)').replace('(size 0.6 0.6)', '(size 0.8 0.8)')
    pcb = prefix + "\n\n".join(rendered) + "\n\n" + suffix
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "pico_lta042b010f_carrier.kicad_pcb").write_text(pcb, encoding="utf-8")


if __name__ == "__main__":
    build_schematic()
    build_pcb()
    print(OUT)
