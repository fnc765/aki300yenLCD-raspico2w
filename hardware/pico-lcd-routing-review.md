# Pico/LCD routing review (2026-09-17)

Raspberry Pi Pico (U1) and LCD connector (J1) routing is complete.  The
previously routed power stage and component placement were preserved.  The
final board has zero unconnected items.

## Routing strategy

- The 0.5 mm-pitch J1 fanout is split toward the upper and lower board areas.
  LCD signals use 0.20 mm tracks, short layer changes, and ordered channels so
  no same-layer crossings remain.
- Pico header gaps and the module RF/antenna copper keepouts are respected.
  The remaining keepout DRC item is the pre-existing H3 footprint, not a new
  track or via.
- +5V and +3V3 trunks use 0.60 mm tracks.  Their short connector/header necks
  use 0.20-0.30 mm where the 0.5 mm pitch or Pico pad spacing requires it.
- +13V8 and -13V8 use 0.30 mm connector necks and 0.60 mm trunks back to the
  completed power stage.  VCPP_ADJ uses 0.20-0.25 mm because it is a
  low-current adjustment node.
- U1 ground pads and every J1 ground pad are connected to the B.Cu LOGIC_GND
  zone.  A short explicit F.Cu bridge prevents the ground plane from being
  split by the ordered signal channels.

The deterministic generator is `route_pico_lcd.py`.  Its final report records
127 route groups and 95 added vias.

## Verification

- KiCad 9 ERC: 0 violations.
- KiCad 9 DRC: 0 unconnected items.
- Routing-related DRC types are all zero: shorting items, track crossings,
  clearance, hole clearance, copper-edge clearance, dangling tracks/vias, and
  solder-mask bridges.
- The 13 remaining board-level DRC items pre-date this routing work: nine
  library-footprint mismatches, two silkscreen-to-edge clearances, one
  silkscreen overlap, and the existing H3 item in the Pico RF keepout.
- U1 and J1 footprint placement and pad/net definitions are not modified by
  the routing generator.

Evidence is stored in
`hardware/review/pico-lcd-routing-2026-09-17/`, including the routing report,
ERC/DRC JSON, validation summary, KiCad SVG, and KiCad 3D render.

These checks validate the design files, not operation on assembled hardware.
LCD timing margin, display quality, power-rail ripple, temperature, and EMI
remain unmeasured.

The follow-up [design-rule and board-edge refinement](design-rule-refinement.md)
adds a 1.0 mm edge margin for ordinary tracks and vias and records the updated
DRC evidence.
