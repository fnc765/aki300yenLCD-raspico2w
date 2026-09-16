# PCB design-rule and board-edge refinement (2026-09-17)

The routed PCB has been revised so ordinary tracks and vias remain at least
1.0 mm from the finished board edge.  The 12 copper-edge violations exposed by
the new rule were reduced to zero without introducing unconnected items,
shorts, track crossings, or ordinary clearance violations.

## Research basis

The board is treated as a generic low-cost, two-layer, 1 oz copper design.
The rules intentionally use more margin than the published manufacturing
minimums:

- KiCad recommends setting Board Setup constraints to the selected fabricator's
  absolute capabilities and using custom rules for stricter design intent.
  Its copper-to-edge check covers tracks, vias, pads, and board cutouts.
  Reference: [KiCad PCB Editor documentation](https://docs.kicad.org/master/en/pcbnew/pcbnew.html).
- JLCPCB publishes a 0.20 mm minimum copper clearance to a routed edge and a
  standard routing tolerance of about +/-0.20 mm.  Its manufacturing guidance
  recommends at least a 0.30 mm safety margin from milled edges.
  References: [JLCPCB capabilities](https://jlcpcb.com/capabilities/Capab) and
  [JLCPCB edge-damage guidance](https://jlcpcb.com/blog/how-to-avoid-damaged-traces-and-pads).
- PCBWay also specifies at least 0.20 mm between copper and the board outline.
  Reference: [PCBWay board-outline guidance](https://www.pcbway.com/helpcenter/board_outline_issues/The_distance_between_the_trace_and_board_outline_is_less_than_0_20mm.html).
- TI, NXP, and Analog Devices emphasize short return paths, a stable reference
  plane, avoiding reference-plane splits, and keeping high-speed signals away
  from board edges.  References:
  [TI high-speed layout guide](https://www.ti.com/lit/ug/sllu149e/sllu149e.pdf),
  [TI reduced-EMI guidance](https://www.ti.com/lit/an/szza009/szza009.pdf),
  [NXP EMC guidelines](https://www.nxp.com/docs/en/application-note/AN2321.pdf),
  and [Analog Devices EMI guidance](https://ez.analog.com/interface-isolation/a/documents/c/adm3251e-faq/DO16684/adm3251e---prevent---minimize-emi-isses).

## Adopted rules

- Global copper-to-edge minimum: 0.50 mm.  This remains the absolute board
  constraint for intentional edge pads and copper pours.
- Preferred track/via-to-edge minimum: 1.00 mm, enforced by
  `pico_lta042b010f_carrier.kicad_dru`.
- Minimum track width, copper clearance, and connection width: 0.20 mm.  The
  0.20 mm signal width is retained only where required by the 0.5 mm-pitch LCD
  connector.
- Minimum via geometry: 0.50/0.30 mm pad/drill.  The default remains
  0.60/0.30 mm; 0.50/0.30 mm is used for the fine-pitch connector fanout.
- Hole-to-copper and hole-to-hole minima: 0.25 mm.
- The LOGIC_GND pour is inset to 1.00 mm at the outer board edge.
- The POWER_STAGE_GND pour remains 0.60 mm from the outer edge.  A tested
  1.00 mm inset separated the pour into two islands around the edge-mounted
  +13 V terminal, so continuous ground return takes precedence here.  The
  0.50 mm global fabrication rule still protects this intentional exception.

## Geometry changes

- Moved the lower LCD fanout inward; its nearest 0.20 mm track now has
  1.40 mm copper-to-edge clearance.
- Replaced the +5 V top-notch crossing with an inset dogleg.  Its 0.60 mm
  trunk and via meet the 1.00 mm routed-copper edge rule.
- Shifted the +3V3 notch-side trunk, LCD_R4 bridge, and -13V8 indicator feed
  inward while preserving the established routing topology.
- Kept the B.Cu ground reference continuous instead of creating a split return
  plane solely to maximize zone-to-edge distance.

The deterministic migration is implemented by `refine_edge_routing.py`;
the fresh-board generators `route_power_stage.py` and
`route_pico_lcd.py` contain the same final coordinates and rules.

## Verification

- Before refinement: 25 DRC violations, including 12 copper-edge violations;
  0 unconnected items.
- After refinement: 13 known non-routing DRC items; 0 copper-edge violations;
  0 unconnected items.
- After refinement: 0 shorts, track crossings, ordinary clearance errors,
  hole-clearance errors, dangling tracks/vias, or solder-mask bridges.
- KiCad 9 ERC: 0 violations.

Evidence is stored in
`hardware/review/design-rule-refinement-2026-09-17/`, including before/after
DRC JSON, ERC JSON, the transformation report, validation summary, SVG, and 3D
render.

These checks validate the design files and geometry only.  Fabrication,
assembled-board power integrity, LCD timing margin, emissions, immunity, and
thermal behavior remain unverified.
