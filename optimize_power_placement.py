"""Deterministic bounded placement search; geometry and pad pins come from KiCad.

The score is a placement heuristic, not an electrical simulation. Review the
native plots and DRC before applying a result. All distances are in mm.
"""
import argparse
import json
import math
import random
from pathlib import Path

import pcbnew
from place_power_stage import BOARD, PLACEMENT, xy

MOVABLE = ['U3', 'L1', 'C6', 'R7', 'R8', 'D4', 'C8', 'C7', 'R9', 'R10']
LINKS = [
    ('U3.1', 'D4.2', 6), ('D4.1', 'C8.1', 5), ('C8.2', 'U3.2', 6),
    ('L1.2', 'U3.1', 5), ('R7.2', 'U3.7', 8), ('R7.2', 'L1.1', 3),
    ('C6.1', 'U3.6', 2), ('C6.1', 'R7.1', 3), ('C6.2', 'U3.2', 5),
    ('C7.1', 'U3.3', 5), ('C7.2', 'U3.4', 3),
    ('R9.2', 'U3.5', 5), ('R10.1', 'U3.5', 5), ('R9.2', 'R10.1', 3),
    ('R10.2', 'U3.4', 1), ('R9.1', 'C8.1', 1),
    ('R8.2', 'U3.8', 3), ('R8.1', 'C6.1', 1),
    ('J2.1', 'C6.1', 1), ('J2.2', 'C6.2', 1),
    ('J3.1', 'C8.1', 1), ('J3.2', 'C8.2', 1),
    ('C9.1', 'C8.1', 1), ('D6.1', 'C8.2', 1),
]
ANGLES = (0, 90, 180, 270)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--steps', type=int, default=40000)
    ap.add_argument('--seed', type=int, default=8)
    ap.add_argument('--start', type=Path)
    ap.add_argument('--temperature', type=float, default=1500)
    args = ap.parse_args()
    rng = random.Random(args.seed)
    b = pcbnew.LoadBoard(str(BOARD))
    fs = {f.GetReference(): f for f in b.GetFootprints()}
    layers = pcbnew.LSET()
    layers.AddLayer(pcbnew.F_CrtYd)
    geom = {}
    for ref, f in fs.items():
        if ref not in PLACEMENT and ref not in ['J1', 'H1', 'H2']:
            continue  # Main search is confined to the left 43 mm, away from Pico.
        f.SetPosition(pcbnew.VECTOR2I(0, 0))
        for angle in ANGLES:
            f.SetOrientationDegrees(angle)
            bb = f.GetLayerBoundingBox(layers)
            if bb.GetWidth() <= 0:
                bb = f.GetBoundingBox(False, False)
            rect = tuple(pcbnew.ToMM(v) for v in
                         [bb.GetX(), bb.GetY(), bb.GetRight(), bb.GetBottom()])
            pads = {p.GetNumber(): xy(p.GetPosition()) for p in f.Pads()}
            geom[ref, angle] = (rect, pads)
    state = {k: tuple(v) for k, v in PLACEMENT.items()}
    state.update({'J1': (37.75, 42.65, 180), 'H1': (4, 4, 0), 'H2': (4, 48, 0)})
    if args.start:
        state.update({k: tuple(v) for k, v in json.loads(args.start.read_text()).items()})
    refs = list(state)

    def cost(state):
        rects, pads = {}, {}
        score = 0.0
        for ref, (x, y, a) in state.items():
            r, ps = geom[ref, a]
            rects[ref] = (r[0] + x, r[1] + y, r[2] + x, r[3] + y)
            pads.update({ref + '.' + n: (p[0] + x, p[1] + y) for n, p in ps.items()})
            if ref in MOVABLE:
                l, t, rr, bb = rects[ref]
                excess = max(2-l, 0) + max(rr-60, 0) + max(2-t, 0) + max(bb-38.3, 0)
                notch = max(min(rr, 50.5) - max(l, 24.5), 0) * max(10.8-t, 0)
                score += 10000 * excess + 2000 * notch
        for i, ref in enumerate(refs):
            for other in refs[i+1:]:
                if ref not in MOVABLE and other not in MOVABLE:
                    continue
                a, b = rects[ref], rects[other]
                # Courtyards already include a margin; add 0.6 mm for tools/routing.
                dx = min(a[2], b[2]) - max(a[0], b[0]) + 0.6
                dy = min(a[3], b[3]) - max(a[1], b[1]) + 0.6
                if dx > 0 and dy > 0:
                    score += 2000 + 2500 * min(dx, dy) + 1500 * dx * dy
        for a, b, w in LINKS:
            score += math.dist(pads[a], pads[b]) * w
        score += 40 * max(0, math.dist(pads['R7.2'], pads['U3.7']) - 8)
        score += 20 * max(0, math.dist(pads['L1.2'], pads['U3.1']) - 10)
        # Reserve a direct high-current corridor; sensitive components must not
        # occupy the proposed switch-node segment or its 1 mm surrounding band.
        for a, b in [('L1.2', 'U3.1'), ('D4.2', 'U3.1')]:
            start, end = pads[a], pads[b]
            for sensitive in ['R9', 'R10', 'C7']:
                left, top, right, bottom = rects[sensitive]
                for j in range(1, 20):
                    px = start[0] + (end[0]-start[0])*j/20
                    py = start[1] + (end[1]-start[1])*j/20
                    if left-1 < px < right+1 and top-1 < py < bottom+1:
                        score += 750
                        break
        # Minimize the area bounded by the boost rectifier/output pulsed-current loop.
        loop = [pads[p] for p in ['U3.1', 'D4.2', 'D4.1', 'C8.1', 'C8.2', 'U3.2']]
        area = abs(sum(loop[i][0]*loop[(i+1)%6][1] - loop[(i+1)%6][0]*loop[i][1]
                       for i in range(6))) / 2
        score += area * 0.4
        return score

    current = cost(state)
    best_score, best = current, state.copy()
    for step in range(args.steps):
        temperature = args.temperature * (0.0001 ** (step / args.steps))
        ref = rng.choice(MOVABLE)
        old = state[ref]
        x, y, a = old
        if rng.random() < 0.18:
            a2 = rng.choice(ANGLES)
            r1, r2 = geom[ref, a][0], geom[ref, a2][0]
            x += (r1[0]+r1[2]-r2[0]-r2[2])/2
            y += (r1[1]+r1[3]-r2[1]-r2[3])/2
            a = a2
        else:
            delta = rng.choice([0.25, 0.5, 1, 2, 4, 8, 12])
            x += rng.choice([-delta, 0, delta])
            y += rng.choice([-delta, 0, delta])
        state[ref] = (round(x*4)/4, round(y*4)/4, a)
        proposal = cost(state)
        if proposal <= current or rng.random() < math.exp(min(0, (current-proposal)/temperature)):
            current = proposal
            if current < best_score:
                best_score, best = current, state.copy()
        else:
            state[ref] = old
    output = {ref: best[ref] for ref in PLACEMENT}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps({'seed': args.seed, 'steps': args.steps, 'score': best_score,
                      'placement': {ref: best[ref] for ref in MOVABLE}}, indent=2))


if __name__ == '__main__':
    main()
