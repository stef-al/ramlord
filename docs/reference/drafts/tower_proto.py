"""RAMlord tower layout prototype.

Implements the unit-grid layout from the spec on sample data and renders a
reference image. Pure functions first (allocation), drawing second.
"""
from PIL import Image
import sys

# ---------------------------------------------------------------- font 5x7
GLYPHS = {
"A": [".###.","#...#","#...#","#####","#...#","#...#","#...#"],
"B": ["####.","#...#","#...#","####.","#...#","#...#","####."],
"C": [".###.","#...#","#....","#....","#....","#...#",".###."],
"D": ["####.","#...#","#...#","#...#","#...#","#...#","####."],
"E": ["#####","#....","#....","####.","#....","#....","#####"],
"F": ["#####","#....","#....","####.","#....","#....","#...."],
"G": [".###.","#...#","#....","#.###","#...#","#...#",".###."],
"H": ["#...#","#...#","#...#","#####","#...#","#...#","#...#"],
"I": [".###.","..#..","..#..","..#..","..#..","..#..",".###."],
"J": ["..###","...#.","...#.","...#.","...#.","#..#.",".##.."],
"K": ["#...#","#..#.","#.#..","##...","#.#..","#..#.","#...#"],
"L": ["#....","#....","#....","#....","#....","#....","#####"],
"M": ["#...#","##.##","#.#.#","#.#.#","#...#","#...#","#...#"],
"N": ["#...#","#...#","##..#","#.#.#","#..##","#...#","#...#"],
"O": [".###.","#...#","#...#","#...#","#...#","#...#",".###."],
"P": ["####.","#...#","#...#","####.","#....","#....","#...."],
"Q": [".###.","#...#","#...#","#...#","#.#.#","#..#.",".##.#"],
"R": ["####.","#...#","#...#","####.","#.#..","#..#.","#...#"],
"S": [".####","#....","#....",".###.","....#","....#","####."],
"T": ["#####","..#..","..#..","..#..","..#..","..#..","..#.."],
"U": ["#...#","#...#","#...#","#...#","#...#","#...#",".###."],
"V": ["#...#","#...#","#...#","#...#","#...#",".#.#.","..#.."],
"W": ["#...#","#...#","#...#","#.#.#","#.#.#","#.#.#",".#.#."],
"X": ["#...#","#...#",".#.#.","..#..",".#.#.","#...#","#...#"],
"Y": ["#...#","#...#",".#.#.","..#..","..#..","..#..","..#.."],
"Z": ["#####","....#","...#.","..#..",".#...","#....","#####"],
"0": [".###.","#...#","#..##","#.#.#","##..#","#...#",".###."],
"1": ["..#..",".##..","..#..","..#..","..#..","..#..",".###."],
"2": [".###.","#...#","....#","...#.","..#..",".#...","#####"],
"3": ["#####","...#.","..#..","...#.","....#","#...#",".###."],
"4": ["...#.","..##.",".#.#.","#..#.","#####","...#.","...#."],
"5": ["#####","#....","####.","....#","....#","#...#",".###."],
"6": ["..##.",".#...","#....","####.","#...#","#...#",".###."],
"7": ["#####","....#","...#.","..#..",".#...",".#...",".#..."],
"8": [".###.","#...#","#...#",".###.","#...#","#...#",".###."],
"9": [".###.","#...#","#...#",".####","....#","...#.",".##.."],
" ": [".....",".....",".....",".....",".....",".....","....."],
".": [".....",".....",".....",".....",".....",".##..",".##.."],
",": [".....",".....",".....",".....",".##..","..#..",".#..."],
":": [".....",".##..",".##..",".....",".##..",".##..","....."],
"-": [".....",".....",".....","#####",".....",".....","....."],
"+": [".....","..#..","..#..","#####","..#..","..#..","....."],
"/": [".....","....#","...#.","..#..",".#...","#....","....."],
"$": ["..#..",".####","#.#..",".###.","..#.#","####.","..#.."],
"%": ["##...","##..#","...#.","..#..",".#...","#..##","...##"],
"(": ["...#.","..#..",".#...",".#...",".#...","..#..","...#."],
")": [".#...","..#..","...#.","...#.","...#.","..#..",".#..."],
"!": ["..#..","..#..","..#..","..#..","..#..",".....","..#.."],
"?": [".###.","#...#","....#","...#.","..#..",".....","..#.."],
"'": [".##..","..#..",".#...",".....",".....",".....","....."],
"|": [".....",".....",".....",".##..",".##..",".....","....."],  # middle dot
"=": [".....",".....","#####",".....","#####",".....","....."],
"&": [".##..","#..#.","#.#..",".#...","#.#.#","#..#.",".##.#"],
"_": [".....",".....",".....",".....",".....",".....","#####"],
">": ["#....",".#...","..#..","...#.","..#..",".#...","#...."],
"^": ["..#..",".###.","#.#.#","..#..","..#..","..#..","..#.."],  # up arrow
"~": [".....",".....",".#...","#.#.#","...#.",".....","....."],  # marks an estimate
}
ADV = 6


def hexc(s):
    s = s.lstrip("#")
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


class Frame:
    """An RGB framebuffer with the handful of primitives the app needs."""

    def __init__(self, w, h, color):
        self.w, self.h = w, h
        self.img = Image.new("RGB", (w, h), color)
        self.px = self.img.load()

    def rect(self, x0, y0, x1, y1, c):  # inclusive
        for y in range(max(0, y0), min(self.h, y1 + 1)):
            for x in range(max(0, x0), min(self.w, x1 + 1)):
                self.px[x, y] = c

    def hline(self, x0, x1, y, c):
        self.rect(x0, y, x1, y, c)

    def vline(self, x, y0, y1, c):
        self.rect(x, y0, x, y1, c)

    def text(self, x, y, s, c, shadow=None):
        for ch in s.upper():
            g = GLYPHS.get(ch, GLYPHS["?"])
            for r, row in enumerate(g):
                for k, bit in enumerate(row):
                    if bit == "#":
                        if shadow is not None and 0 <= x + k + 1 < self.w and 0 <= y + r + 1 < self.h:
                            self.px[x + k + 1, y + r + 1] = shadow
            for r, row in enumerate(g):
                for k, bit in enumerate(row):
                    if bit == "#" and 0 <= x + k < self.w and 0 <= y + r < self.h:
                        self.px[x + k, y + r] = c
            x += ADV

    def text_big(self, x, y, s, c, k=2):
        for ch in s.upper():
            g = GLYPHS.get(ch, GLYPHS["?"])
            for r, row in enumerate(g):
                for q, bit in enumerate(row):
                    if bit == "#":
                        self.rect(x + q * k, y + r * k, x + q * k + k - 1, y + r * k + k - 1, c)
            x += ADV * k

    def text_center(self, x0, x1, y, s, c):
        self.text(x0 + ((x1 - x0 + 1) - len(s) * ADV) // 2, y, s, c)

    def text_right(self, x_right, y, s, c):
        self.text(x_right - len(s) * ADV + 1, y, s, c)


# ---------------------------------------------------------------- palette
P = {k: hexc(v) for k, v in {
    "sky": "#1b1f3b", "sky2": "#232848", "star": "#8f97c9", "moon": "#f4f1de",
    "outline": "#0d0e1c", "wall": "#3b3f5c", "roof": "#55506e", "roof2": "#6b6585",
    "found": "#2b2d42", "street": "#15172b", "curb": "#3b3f5c",
    "vac_wall": "#20233f", "vac_win": "#2b2f55",
    "staff_wall": "#4a4e69", "staff_win": "#8a8fa8", "staff_dark": "#3a3d55",
    "cream": "#f4f1de", "dim": "#9aa0c3", "gold": "#ffd166", "alert": "#ff5d5d",
    "good": "#6ee7a0", "panel": "#14162b", "panel_line": "#3b3f5c", "dark_win": "#191b33",
}.items()}

HUES = [  # (wall, lit window, mortar)
    ("#c94a4a", "#ffc2b8"), ("#d9822b", "#ffe0a8"), ("#3f8fd9", "#c4e2ff"),
    ("#3fae6b", "#bff2d2"), ("#8f5ae0", "#dcc6ff"), ("#2fa3a3", "#b4f0ec"),
    ("#c94fb5", "#f7c0ee"), ("#b89a2a", "#fff1a8"), ("#5a6ee0", "#c6ceff"),
    ("#e0608f", "#ffc6d8"), ("#7fae3a", "#dff2b0"), ("#b08a5a", "#f0d8b6"),
]
HUES = [(hexc(a), hexc(b)) for a, b in HUES]


def darker(c, f=0.72):
    return tuple(int(v * f) for v in c)


# ---------------------------------------------------------------- layout (pure)
COLS, FLOORS = 16, 16
UNITS = COLS * FLOORS


MAX_NAMED = 40


def allocate(capacity, staff, tenants, vacant):
    """Split the building's 256 units between staff, tenants, others and vacant.

    All arithmetic is in integer bytes so the result is identical everywhere.
    tenants: list of (id, bytes). Returns (fill_order, merged_ids) where
    fill_order is [(id, units)] from the ground floor up: staff, named tenants
    by bytes descending, others, vacant.
    """
    capacity, staff, vacant = int(capacity), int(staff), int(vacant)
    ranked = sorted(((i, int(b)) for i, b in tenants), key=lambda t: (-t[1], str(t[0])))
    named = [(i, b) for i, b in ranked if 2 * b * UNITS >= capacity][:MAX_NAMED]
    named_ids = {i for i, _ in named}
    merged = [(i, b) for i, b in ranked if i not in named_ids]
    entries = [("staff", staff)] + named
    if merged:
        entries.append(("others", sum(b for _, b in merged)))
    entries.append(("vacant", vacant))
    units = [b * UNITS // capacity for _, b in entries]
    frac = [b * UNITS % capacity for _, b in entries]
    left = UNITS - sum(units)
    order = sorted(range(len(entries)), key=lambda k: (-frac[k], -entries[k][1], k))
    for k in order[:max(left, 0)]:
        units[k] += 1
    # anything with memory keeps at least one unit, paid for by the largest entry
    for k, (ident, b) in enumerate(entries):
        if b > 0 and units[k] == 0 and ident != "vacant":
            donor = max(range(len(units)), key=lambda q: units[q])
            if units[donor] > 1:
                units[donor] -= 1
                units[k] = 1
    # rounding drift (inputs that do not sum to capacity) lands on vacant
    drift = UNITS - sum(units)
    units[-1] = max(0, units[-1] + drift)
    while sum(units) > UNITS:
        donor = max(range(len(units)), key=lambda q: units[q])
        units[donor] -= 1
    return [(entries[k][0], units[k]) for k in range(len(entries))], [i for i, _ in merged]


def unit_owner_grid(alloc):
    owners = []
    for ident, n in alloc:
        owners += [ident] * n
    assert len(owners) == UNITS, len(owners)
    return owners  # index 0 = ground floor, leftmost


def row_segments(owners, ident):
    """Per-floor contiguous runs for one owner: list of (floor, col0, col1)."""
    segs = []
    for f in range(FLOORS):
        cols = [c for c in range(COLS) if owners[f * COLS + c] == ident]
        if cols:
            segs.append((f, cols[0], cols[-1]))
    return segs


# ---------------------------------------------------------------- sample data
GB = 1.0
CAPACITY = 32.0
PRICE = 14.34
SAMPLE = [  # name, GB, process count, flags
    ("Chrome", 6.4, 47, set()),
    ("Discord", 2.9, 6, {"leak"}),
    ("VS Code", 2.2, 14, set()),
    ("Teams", 1.6, 9, {"nothome"}),
    ("Steam", 0.9, 8, {"walls"}),
    ("Slack", 0.8, 5, {"nothome"}),
    ("Spotify", 0.6, 6, set()),
    ("WhatsApp", 0.5, 7, {"squatter"}),
    ("Copilot", 0.5, 4, {"squatter"}),
    ("OneDrive", 0.3, 2, {"walls"}),
    ("Terminal", 0.04, 3, set()),
    ("Notepad", 0.03, 1, set()),
]
VACANT = 9.8
SELF_MB = 31


def build_scene():
    tenants = sorted(SAMPLE, key=lambda t: (-t[1], t[0]))
    tsum = sum(t[1] for t in tenants)
    staff = CAPACITY - VACANT - tsum
    MB = 1024 * 1024
    to_b = lambda gb: round(gb * 1024) * MB
    alloc, small = allocate(to_b(CAPACITY), to_b(CAPACITY) - to_b(VACANT) - sum(to_b(t[1]) for t in tenants),
                            [(t[0], to_b(t[1])) for t in tenants], to_b(VACANT))
    return tenants, staff, alloc, small


# ---------------------------------------------------------------- render
W, H = 384, 240
CW, CH = 12, 11
BX, BY = 16, 30  # interior top-left


def cell_xy(floor, col):
    return BX + col * CW, BY + (FLOORS - 1 - floor) * CH


def render(path, scale=3):
    tenants, staff, alloc, small = build_scene()
    owners = unit_owner_grid(alloc)
    info = {t[0]: t for t in tenants}
    color = {}
    for k, (ident, n) in enumerate([a for a in alloc if a[0] not in ("staff", "vacant", "others")]):
        color[ident] = HUES[k % len(HUES)]

    fr = Frame(W, H, P["sky"])
    # sky
    fr.rect(0, 120, W - 1, H - 1, P["sky2"])
    for (sx, sy) in [(6, 9), (228, 6), (250, 222), (300, 4), (340, 9), (372, 3), (216, 60), (218, 150), (4, 100), (7, 170), (376, 226), (220, 200)]:
        fr.px[sx, sy] = P["star"]
    # street
    fr.rect(0, 213, 219, 215, P["street"])
    fr.hline(0, 219, 212, P["curb"])

    # building shell
    x0, x1 = BX - 4, BX + COLS * CW + 3
    yt, yb = BY, BY + FLOORS * CH - 1
    fr.rect(x0, yt - 12, x1, yt - 1, P["roof"])
    fr.hline(x0, x1, yt - 12, P["roof2"])
    fr.rect(x0 - 2, yt - 3, x1 + 2, yt - 1, P["roof2"])
    fr.rect(x0, yt, x1, yb, P["wall"])
    fr.rect(x0 - 1, yb + 1, x1 + 1, yb + 6, P["found"])
    # antenna and roof sign
    fr.vline(x1 - 14, yt - 24, yt - 13, P["roof2"])
    fr.px[x1 - 14, yt - 25] = P["alert"]
    fr.text(x0 + 6, yt - 23, "RAMLORD", P["gold"], shadow=P["outline"])

    # label strips: decide per tenant before drawing units
    strips = {}
    for ident in color:
        segs = row_segments(owners, ident)
        f, c0, c1 = max(segs, key=lambda s: (s[2] - s[1], -s[0]))
        width_px = (c1 - c0 + 1) * CW - 4
        name = ident.upper()
        rent = f"${info[ident][1] * PRICE:.0f}"
        full = f"{name} {rent}"
        if len(full) * ADV <= width_px:
            label = full
        elif len(name) * ADV <= width_px:
            label = name
        else:
            label = ""
        if label:
            covered = -(-(len(label) * ADV + 3) // CW)
            strips[ident] = (f, c0, covered, label)

    # units
    for idx, ident in enumerate(owners):
        f, c = divmod(idx, COLS)
        x, y = cell_xy(f, c)
        if ident == "vacant":
            wall, win = P["vac_wall"], P["vac_win"]
        elif ident == "staff":
            wall, win = P["staff_wall"], P["staff_win"]
        elif ident == "others":
            wall, win = hexc("#6b6f8f"), hexc("#d5d8ea")
        else:
            wall, win = color[ident]
        fr.rect(x, y, x + CW - 1, y + CH - 1, wall)
        in_strip = ident in strips and strips[ident][0] == f and strips[ident][1] <= c < strips[ident][1] + strips[ident][2]
        flags = info[ident][3] if ident in info else set()
        if not in_strip:
            if ident == "staff":
                fr.rect(x + 2, y + 2, x + 8, y + 7, P["staff_dark"])
                fr.hline(x + 2, x + 8, y + 4, win)  # pipes
                fr.px[x + 5, y + 6] = win
            elif "walls" in flags:
                fr.px[x + 3, y + 4] = darker(wall, 0.6)
                fr.px[x + 5, y + 4] = darker(wall, 0.6)
                fr.px[x + 7, y + 4] = darker(wall, 0.6)
                fr.px[x + 3, y + 6] = darker(wall, 0.6)
                fr.px[x + 5, y + 6] = darker(wall, 0.6)
                fr.px[x + 7, y + 6] = darker(wall, 0.6)
            elif "nothome" in flags:
                fr.rect(x + 2, y + 2, x + 8, y + 7, P["dark_win"])
            elif ident == "vacant":
                fr.rect(x + 2, y + 2, x + 8, y + 7, win)
            else:
                fr.rect(x + 2, y + 2, x + 8, y + 7, win)
                fr.vline(x + 5, y + 2, y + 7, darker(wall, 0.85))  # mullion
        # mortar inside one owner: slightly darker than its wall
        fr.hline(x, x + CW - 1, y + CH - 1, darker(wall, 0.8))
        fr.vline(x + CW - 1, y, y + CH - 1, darker(wall, 0.8))

    # borders between different owners
    for idx, ident in enumerate(owners):
        f, c = divmod(idx, COLS)
        x, y = cell_xy(f, c)
        right = owners[idx + 1] if c < COLS - 1 else None
        above = owners[idx + COLS] if f < FLOORS - 1 else None
        if right is not None and right != ident:
            fr.vline(x + CW - 1, y, y + CH - 1, P["outline"])
        if above is not None and above != ident:
            fr.hline(x - (1 if c else 0), x + CW - 1, y - 1, P["outline"])
    # frame around interior
    fr.hline(BX - 1, BX + COLS * CW - 1, BY - 1, P["outline"])
    fr.hline(BX - 1, BX + COLS * CW - 1, yb, P["outline"])
    fr.vline(BX - 1, BY - 1, yb, P["outline"])
    fr.vline(BX + COLS * CW - 1, BY - 1, yb, P["outline"])

    # labels
    for ident, (f, c0, covered, label) in strips.items():
        x, y = cell_xy(f, c0)
        fr.text(x + 3, y + 1, label, P["cream"], shadow=P["outline"])
        flags = info[ident][3]
        if "leak" in flags:
            fr.text(x + 3 + len(label) * ADV + 2, y + 1, "^", P["alert"], shadow=P["outline"])
    # staff label
    segs = row_segments(owners, "staff")
    if segs:
        x, y = cell_xy(0, 0)
        fr.rect(x + 1, y + 1, x + 34, y + 8, P["staff_wall"])
        fr.text(x + 3, y + 1, "STAFF", P["dim"], shadow=P["outline"])
    # vacancy sign on the top vacant floor
    vsegs = row_segments(owners, "vacant")
    if vsegs:
        f, c0, c1 = max(vsegs, key=lambda s: (s[2] - s[1], s[0]))
        if c1 - c0 >= 5:
            x, y = cell_xy(f, c0)
            cx = x + ((c1 - c0 + 1) * CW - 8 * ADV) // 2
            fr.rect(cx - 3, y + 1, cx + 8 * ADV + 1, y + 9, P["vac_wall"])
            fr.text(cx, y + 2, "FOR RENT", P["dim"])

    # status lines under the building
    used = CAPACITY - VACANT
    fr.text(12, 219, f"OCCUPANCY {used / CAPACITY * 100:.0f}%  {VACANT:.1f} GB VACANT", P["dim"])
    fr.text(12, 230, "TEAMS HASN'T BEEN HOME IN 3 DAYS.", P["cream"])

    # ---------------- ledger
    lx0, lx1 = 224, 377
    fr.rect(lx0, 10, lx1, 213, P["panel"])
    for a, b, c_, d in [(lx0, 10, lx1, 10), (lx0, 213, lx1, 213)]:
        fr.hline(a, c_, b, P["panel_line"])
    fr.vline(lx0, 10, 213, P["panel_line"])
    fr.vline(lx1, 10, 213, P["panel_line"])
    fr.text(lx0 + 5, 15, "RENT ROLL", P["gold"])
    fr.text(lx0 + 5, 26, "DDR5 $14.34/GB  NOV 3", P["dim"])
    fr.hline(lx0 + 4, lx1 - 4, 36, P["panel_line"])
    fr.text(lx0 + 13, 40, "TENANT", P["dim"])
    fr.text_right(lx0 + 121, 40, "GB", P["dim"])
    fr.text_right(lx1 - 5, 40, "RENT", P["dim"])
    y = 51
    tag = {"leak": ("LEAK", P["alert"]), "nothome": ("AWAY", P["dim"]), "walls": ("WALLS", P["dim"]), "squatter": ("SQUAT", P["gold"])}
    shown = [t for t in tenants if t[0] in color]
    for t in shown[:10]:
        name, gb, procs, flags = t
        wall, win = color[name]
        fr.rect(lx0 + 5, y, lx0 + 9, y + 6, wall)
        fr.text(lx0 + 13, y, name.upper()[:8], P["cream"])
        for fl in ("leak", "squatter", "nothome", "walls"):
            if fl in flags:
                fr.text(lx0 + 66, y, tag[fl][0], tag[fl][1])
                break
        fr.text_right(lx0 + 121, y, f"{gb:.1f}", P["dim"])
        fr.text_right(lx1 - 5, y, f"${gb * PRICE:.0f}", P["cream"])
        y += 10
    if small:
        gb = sum(info[i][1] for i in small)
        fr.rect(lx0 + 5, y, lx0 + 9, y + 6, hexc("#6b6f8f"))
        fr.text(lx0 + 13, y, f"OTHERS ({len(small)})", P["dim"])
        fr.text_right(lx0 + 121, y, f"{gb:.1f}", P["dim"])
        fr.text_right(lx1 - 5, y, f"${gb * PRICE:.0f}", P["dim"])
        y += 10
    y += 2
    fr.hline(lx0 + 4, lx1 - 4, y, P["panel_line"])
    y += 5
    total = sum(round(t[1] * PRICE) for t in shown[:10]) + (round(sum(info[i][1] for i in small) * PRICE) if small else 0)
    fr.text(lx0 + 5, y, "RENT OWED", P["gold"])
    fr.text_right(lx1 - 5, y, f"${total:.0f}", P["gold"])
    y += 12
    fr.rect(lx0 + 5, y, lx0 + 9, y + 6, P["staff_wall"])
    fr.text(lx0 + 13, y, "STAFF", P["dim"])
    fr.text_right(lx0 + 121, y, f"{staff:.1f}", P["dim"])
    fr.text_right(lx1 - 5, y, f"${staff * PRICE:.0f}", P["dim"])
    y += 10
    fr.rect(lx0 + 5, y, lx0 + 9, y + 6, P["vac_win"])
    fr.text(lx0 + 13, y, "VACANT", P["dim"])
    fr.text_right(lx0 + 121, y, f"{VACANT:.1f}", P["dim"])
    fr.text_right(lx1 - 5, y, f"${VACANT * PRICE:.0f}", P["good"])
    y += 14
    fr.text(lx0 + 5, y, f"MY OFFICE {SELF_MB} MB", P["dim"])
    fr.text_right(lx1 - 5, y, f"${SELF_MB / 1024 * PRICE:.2f}", P["dim"])

    out = fr.img.resize((W * scale, H * scale), Image.NEAREST)
    out.save(path)
    return alloc, small


def render_receipt(path, scale=4):
    """The shareable rent roll: 216x270 logical (4:5), a paper receipt on the night sky."""
    tenants, staff, alloc, small = build_scene()
    RW, RH = 240, 300
    fr = Frame(RW, RH, P["sky"])
    for (sx, sy) in [(8, 12), (12, 280), (230, 20), (228, 120), (10, 150), (232, 268), (12, 66), (229, 200), (6, 230)]:
        fr.px[sx, sy] = P["star"]
    paper, ink, faint = hexc("#f4f1de"), hexc("#1b1f3b"), hexc("#8a8576")
    px0, px1 = 22, 217
    lines = []  # (kind, payload)
    lines.append(("big", "RAMLORD"))
    lines.append(("center", "RENT ROLL"))
    lines.append(("center", "MON 9 NOV 2026"))
    lines.append(("center", "32 GB DDR5 AT $14.34/GB"))
    lines.append(("rule", None))
    shown = [t for t in tenants if t[0] not in small]
    tagw = {"leak": "LEAK", "nothome": "AWAY", "walls": "WALLS", "squatter": "SQUAT"}
    for name, gb, procs, flags in shown:
        tg = next((tagw[f] for f in ("leak", "squatter", "nothome", "walls") if f in flags), "")
        lines.append(("row", (name.upper(), tg, f"{gb:.1f} GB", f"${gb * PRICE:.0f}")))
    if small:
        gb = sum(t[1] for t in tenants if t[0] in small)
        lines.append(("row", (f"OTHERS ({len(small)})", "", f"{gb:.1f} GB", f"${gb * PRICE:.0f}")))
    lines.append(("rule", None))
    total = sum(round(t[1] * PRICE) for t in shown) + (round(sum(t[1] for t in tenants if t[0] in small) * PRICE) if small else 0)
    lines.append(("total", ("RENT OWED", f"${total:.0f}")))
    lines.append(("row", ("VACANT", "", f"{VACANT:.1f} GB", f"${VACANT * PRICE:.0f}")))
    lines.append(("rule", None))
    lines.append(("center", "TEAMS HASN'T BEEN HOME"))
    lines.append(("center", "IN 3 DAYS."))
    lines.append(("gap", None))
    lines.append(("center", "THEY LIVED RENT-FREE."))
    lines.append(("center", "NOT ANYMORE."))
    height = {"big": 20, "center": 10, "rule": 8, "row": 10, "total": 19, "gap": 5}
    body = sum(height[k] for k, _ in lines)
    top = (RH - body - 22) // 2
    bottom = top + body + 16
    fr.rect(px0 + 2, top + 2, px1 + 2, bottom + 2, hexc("#11132a"))  # shadow
    fr.rect(px0, top, px1, bottom, paper)
    # torn edge
    for x in range(px0, px1 + 1):
        if ((x - px0) // 3) % 2 == 0:
            fr.rect(x, bottom - 1, x, bottom + 2, P["sky"])
    y = top + 8
    for kind, payload in lines:
        if kind == "big":
            w = len(payload) * ADV * 2
            fr.text_big(px0 + ((px1 - px0 + 1) - w) // 2, y, payload, ink, 2)
        elif kind == "center":
            fr.text_center(px0, px1, y, payload, ink)
        elif kind == "rule":
            for x in range(px0 + 6, px1 - 5, 4):
                fr.hline(x, x + 1, y + 2, faint)
        elif kind == "row":
            name, tg, gb, usd = payload
            fr.text(px0 + 6, y, name[:10], ink)
            if tg:
                fr.text(px0 + 6 + 10 * ADV + 2, y, tg, hexc("#b23a3a") if tg == "LEAK" else faint)
            fr.text_right(px1 - 40, y, gb, faint)
            fr.text_right(px1 - 6, y, usd, ink)
        elif kind == "total":
            name, usd = payload
            fr.text_big(px0 + 6, y + 1, name, ink, 2)
            fr.text_big(px1 - 5 - len(usd) * ADV * 2 + 2, y + 1, usd, ink, 2)
        y += height[kind]
    out = fr.img.resize((RW * scale, RH * scale), Image.NEAREST)
    out.save(path)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "tower.png"
    alloc, small = render(out)
    print("alloc:", alloc)
    print("others:", small)
    print("sum:", sum(n for _, n in alloc))
    if len(sys.argv) > 2:
        render_receipt(sys.argv[2])
