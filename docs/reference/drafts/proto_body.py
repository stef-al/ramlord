ADV = 6


def hexc(s):
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


# ------------------------------------------------------------ palette (SPEC.md, The tower)
SKY, SKY_LOW, STAR, OUTLINE = hexc("1b1f3b"), hexc("232848"), hexc("8f97c9"), hexc("0d0e1c")
BUILDING, ROOF, ROOF_EDGE, FOUNDATION = hexc("3b3f5c"), hexc("55506e"), hexc("6b6585"), hexc("2b2d42")
STREET = hexc("15172b")
VACANT_WALL, VACANT_WINDOW, DARK_WINDOW = hexc("20233f"), hexc("2b2f55"), hexc("191b33")
STAFF_WALL, STAFF_DETAIL, STAFF_PANEL = hexc("4a4e69"), hexc("8a8fa8"), hexc("3a3d55")
OTHERS_WALL, OTHERS_WINDOW = hexc("6b6f8f"), hexc("d5d8ea")
TEXT, TEXT_DIM, GOLD, ALERT, GOOD = hexc("f4f1de"), hexc("9aa0c3"), hexc("ffd166"), hexc("ff5d5d"), hexc("6ee7a0")
PANEL, PANEL_LINE, PAPER, INK = hexc("14162b"), hexc("3b3f5c"), hexc("f4f1de"), hexc("1b1f3b")
PAPER_FAINT, PAPER_ALERT, PAPER_SHADOW = hexc("8a8576"), hexc("b23a3a"), hexc("11132a")
HUES = [(hexc(a), hexc(b)) for a, b in [  # wall, lit window; hue 1 is HUES[0]
    ("c94a4a", "ffc2b8"), ("d9822b", "ffe0a8"), ("3f8fd9", "c4e2ff"), ("3fae6b", "bff2d2"),
    ("8f5ae0", "dcc6ff"), ("2fa3a3", "b4f0ec"), ("c94fb5", "f7c0ee"), ("b89a2a", "fff1a8"),
    ("5a6ee0", "c6ceff"), ("e0608f", "ffc6d8"), ("7fae3a", "dff2b0"), ("b08a5a", "f0d8b6")]]


def shade(c, percent):
    return tuple(v * percent // 100 for v in c)


# ------------------------------------------------------------ the sample building
MIB = 1 << 20
GIB = 1 << 30
MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]


def sample_building():
    """The demo fixture and first test vector: 32 GB of DDR5 at $14.34 per GB."""
    rows = [  # name, rent MiB, lease MiB, processes, flags
        ("Chrome", 6554, 8192, 47, []),
        ("Discord", 2970, 4198, 6, ["leaker"]),
        ("VS Code", 2253, 3072, 14, []),
        ("Teams", 1638, 2355, 9, ["away"]),
        ("Steam", 922, 1229, 8, ["walls"]),
        ("Slack", 819, 1126, 5, ["away"]),
        ("Spotify", 614, 870, 6, []),
        ("Copilot", 512, 717, 4, ["squatter"]),
        ("WhatsApp", 512, 768, 7, ["squatter"]),
        ("OneDrive", 307, 410, 2, ["walls"]),
        ("Terminal", 41, 72, 3, []),
        ("Notepad", 31, 46, 1, []),
    ]
    capacity, vacant = 32768 * MIB, 10035 * MIB
    tenants = [{"id": name.lower().replace(" ", "") + ".exe", "name": name, "rent_bytes": mib * MIB,
                "lease_bytes": lease * MIB, "process_count": procs, "flags": flags}
               for name, mib, lease, procs, flags in rows]
    return {
        "kind": "building",
        "date": "2026-11-09",
        "memory_type": "DDR5",
        "price": {"per_gb_micros": 14_340_000, "symbol": "$", "as_of": "2026-11-03"},
        "capacity_bytes": capacity,
        "vacant_bytes": vacant,
        "staff_bytes": capacity - vacant - sum(t["rent_bytes"] for t in tenants),
        "own_bytes": 31 * MIB,
        "remark": "TEAMS hasn't been home in 3 days.",
        "tenants": tenants,
    }


# ------------------------------------------------------------ numbers (integers only)
def micros(nbytes, price):
    return nbytes * price["per_gb_micros"] // GIB


def whole_units(nbytes, price):
    """Whole currency units, halves rounded up."""
    return (micros(nbytes, price) + 500_000) // 1_000_000


def money(nbytes, price, cents=False):
    if cents:
        c = (micros(nbytes, price) + 5_000) // 10_000
        return f"{price['symbol']}{c // 100}.{c % 100:02d}"
    return f"{price['symbol']}{whole_units(nbytes, price)}"


def price_text(price):
    m = price["per_gb_micros"]
    return f"{price['symbol']}{m // 1_000_000}.{m % 1_000_000 // 10_000:02d}"


def gb(nbytes):
    """GB to one decimal, halves rounded up."""
    tenths = (nbytes * 10 + GIB // 2) // GIB
    return f"{tenths // 10}.{tenths % 10}"


# ------------------------------------------------------------ dividing the building
COLS, FLOORS = 16, 16
UNITS = COLS * FLOORS
MAX_NAMED = 40
ROLL_ROWS = 10
TAGS = [("leaker", "LEAK"), ("squatter", "SQUAT"), ("away", "AWAY"), ("walls", "WALLS")]  # priority order


def ranked(tenants):
    return sorted(tenants, key=lambda t: (-t["rent_bytes"], t["name"]))


def split_tenants(building):
    """Named tenants (at least half a unit, at most 40) and the ones merged into Others."""
    order = ranked(building["tenants"])
    named = [t for t in order if 2 * t["rent_bytes"] * UNITS >= building["capacity_bytes"]][:MAX_NAMED]
    return named, [t for t in order if t not in named]


def allocate(building):
    """SPEC.md 'Dividing the building'.

    Returns [(owner, units)] from the ground floor up, where owner is
    "staff", a tenant id, "others" or "vacant".
    """
    capacity = building["capacity_bytes"]
    named, merged = split_tenants(building)
    entries = [("staff", building["staff_bytes"])] + [(t["id"], t["rent_bytes"]) for t in named]
    if merged:
        entries.append(("others", sum(t["rent_bytes"] for t in merged)))
    entries.append(("vacant", building["vacant_bytes"]))
    assert sum(b for _, b in entries) == capacity, "staff + tenants + vacant must equal capacity"

    units = [b * UNITS // capacity for _, b in entries]
    remainder = [b * UNITS % capacity for _, b in entries]
    by_remainder = sorted(range(len(entries)), key=lambda k: (-remainder[k], -entries[k][1], k))
    for k in by_remainder[:UNITS - sum(units)]:
        units[k] += 1
    for k, (owner, b) in enumerate(entries):  # anything with memory keeps one unit
        if b > 0 and units[k] == 0 and owner != "vacant":
            donor = max(range(len(units)), key=lambda q: units[q])
            units[donor] -= 1
            units[k] = 1
    return [(entries[k][0], units[k]) for k in range(len(entries))]


def unit_owners(fill_order):
    owners = []
    for owner, n in fill_order:
        owners += [owner] * n
    return owners  # index 0 is the ground floor, leftmost unit


def floor_segments(owners, owner):
    """One (floor, first_col, last_col) per floor the owner occupies."""
    segs = []
    for f in range(FLOORS):
        cols = [c for c in range(COLS) if owners[f * COLS + c] == owner]
        if cols:
            segs.append((f, cols[0], cols[-1]))
    return segs


def assign_hues(named):
    """New tenants take the least used hue, in fill order, ties in palette order."""
    used, hue = [0] * len(HUES), {}
    for t in named:
        k = min(range(len(HUES)), key=lambda q: (used[q], q))
        used[k] += 1
        hue[t["id"]] = k
    return hue


def roll_rows(building):
    """The 10 largest tenants, and everyone else for the Others row."""
    named, merged = split_tenants(building)
    return named[:ROLL_ROWS], named[ROLL_ROWS:] + merged


def tag_of(tenant):
    return next((label for flag, label in TAGS if flag in tenant["flags"]), "")


def wrap_balanced(text, width):
    """Lines of at most `width` characters, as few as possible, balanced in length."""
    words = text.upper().split(" ")
    count = -(-len(" ".join(words)) // width)
    while True:
        best = None

        def search(start, lines):
            nonlocal best
            if len(lines) == count - 1:
                last = " ".join(words[start:])
                if start < len(words) and len(last) <= width:
                    done = lines + [last]
                    spread = max(map(len, done)) - min(map(len, done))
                    if best is None or spread < best[0]:
                        best = (spread, done)
                return
            for end in range(start + 1, len(words)):
                line = " ".join(words[start:end])
                if len(line) > width:
                    break
                search(end, lines + [line])

        search(0, [])
        if best:
            return best[1]
        count += 1


# ------------------------------------------------------------ drawing
class Frame:
    def __init__(self, w, h, color):
        self.w, self.h = w, h
        self.img = Image.new("RGB", (w, h), color)
        self.px = self.img.load()

    def rect(self, x0, y0, x1, y1, c):  # inclusive corners
        for y in range(max(0, y0), min(self.h, y1 + 1)):
            for x in range(max(0, x0), min(self.w, x1 + 1)):
                self.px[x, y] = c

    def dot(self, x, y, c):
        self.rect(x, y, x, y, c)

    def text(self, x, y, s, c, shadow=None, scale=1):
        for ch in s.upper():
            glyph = GLYPHS.get(ch, GLYPHS["?"])
            for offset, color in ((1, shadow), (0, c)):  # shadow first, one pixel down and right
                if color is None:
                    continue
                for r, row in enumerate(glyph):
                    for k, bit in enumerate(row):
                        if bit == "#":
                            px, py = x + k * scale + offset, y + r * scale + offset
                            self.rect(px, py, px + scale - 1, py + scale - 1, color)
            x += ADV * scale

    def text_right(self, x_right, y, s, c, scale=1):
        """The last glyph's right edge lands on x_right."""
        self.text(x_right - (len(s) * ADV - 1) * scale + 1, y, s, c, scale=scale)

    def text_center(self, x0, x1, y, s, c, scale=1):
        self.text(x0 + ((x1 - x0 + 1) - len(s) * ADV * scale) // 2, y, s, c, scale=scale)


W, H = 384, 240
CELL_W, CELL_H = 12, 11
GRID_X, GRID_Y = 16, 30


def cell_xy(floor, col):
    return GRID_X + col * CELL_W, GRID_Y + (FLOORS - 1 - floor) * CELL_H


def render_tower(building):
    price = building["price"]
    by_id = {t["id"]: t for t in building["tenants"]}
    named, _ = split_tenants(building)
    fill_order = allocate(building)
    owners = unit_owners(fill_order)
    hue = assign_hues(named)

    fr = Frame(W, H, SKY)
    fr.rect(0, 120, W - 1, H - 1, SKY_LOW)
    for sx, sy in [(6, 9), (228, 6), (300, 4), (340, 9), (372, 3), (216, 60), (218, 150), (4, 100), (7, 170), (220, 200)]:
        fr.dot(sx, sy, STAR)
    fr.rect(0, 212, 219, 212, BUILDING)  # kerb
    fr.rect(0, 213, 219, 215, STREET)

    # shell: roof, walls, foundation, sign, antenna
    x0, x1 = GRID_X - 4, GRID_X + COLS * CELL_W + 3
    top, bottom = GRID_Y, GRID_Y + FLOORS * CELL_H - 1
    fr.rect(x0, top - 12, x1, top - 1, ROOF)
    fr.rect(x0, top - 12, x1, top - 12, ROOF_EDGE)
    fr.rect(x0 - 2, top - 3, x1 + 2, top - 1, ROOF_EDGE)
    fr.rect(x0, top, x1, bottom, BUILDING)
    fr.rect(x0 - 1, bottom + 1, x1 + 1, bottom + 6, FOUNDATION)
    fr.rect(x1 - 14, top - 24, x1 - 14, top - 13, ROOF_EDGE)
    fr.dot(x1 - 14, top - 25, ALERT)
    fr.text(x0 + 6, top - 23, "RAMLORD", GOLD, shadow=OUTLINE)

    # label strips: NAME RENT if it fits the tenant's widest floor segment, else NAME, else nothing
    strips = {}
    for t in named:
        floor, c0, c1 = max(floor_segments(owners, t["id"]), key=lambda s: (s[2] - s[1], -s[0]))
        room = (c1 - c0 + 1) * CELL_W - 4
        arrow = "^" if "leaker" in t["flags"] else ""  # the ^ glyph is an up arrow
        extra = 2 + ADV if arrow else 0  # the arrow sits 2 pixels after the label
        name = t["name"].upper()
        full = f"{name} {money(t['rent_bytes'], price)}"
        label = next((s for s in (full, name) if len(s) * ADV + extra <= room), None)
        if label:
            cells = -(-(len(label) * ADV + extra + 3) // CELL_W)
            strips[t["id"]] = (floor, c0, cells, label, arrow)

    for idx, owner in enumerate(owners):
        floor, col = divmod(idx, COLS)
        x, y = cell_xy(floor, col)
        flags = by_id[owner]["flags"] if owner in by_id else []
        if owner == "vacant":
            wall, window = VACANT_WALL, VACANT_WINDOW
        elif owner == "staff":
            wall, window = STAFF_WALL, STAFF_DETAIL
        elif owner == "others":
            wall, window = OTHERS_WALL, OTHERS_WINDOW
        else:
            wall, window = HUES[hue[owner]]
        fr.rect(x, y, x + CELL_W - 1, y + CELL_H - 1, wall)
        strip = strips.get(owner)
        in_strip = strip is not None and strip[0] == floor and strip[1] <= col < strip[1] + strip[2]
        if not in_strip:
            if owner == "staff":  # a service panel with a pipe
                fr.rect(x + 2, y + 2, x + 8, y + 7, STAFF_PANEL)
                fr.rect(x + 2, y + 4, x + 8, y + 4, window)
                fr.dot(x + 5, y + 6, window)
            elif "walls" in flags:  # no window, six vent dots
                for dx in (3, 5, 7):
                    for dy in (4, 6):
                        fr.dot(x + dx, y + dy, shade(wall, 60))
            elif "away" in flags:
                fr.rect(x + 2, y + 2, x + 8, y + 7, DARK_WINDOW)
            elif owner == "vacant":
                fr.rect(x + 2, y + 2, x + 8, y + 7, window)
            else:  # lit two-pane window
                fr.rect(x + 2, y + 2, x + 8, y + 7, window)
                fr.rect(x + 5, y + 2, x + 5, y + 7, shade(wall, 85))
        fr.rect(x, y + CELL_H - 1, x + CELL_W - 1, y + CELL_H - 1, shade(wall, 80))  # mortar
        fr.rect(x + CELL_W - 1, y, x + CELL_W - 1, y + CELL_H - 1, shade(wall, 80))

    for idx, owner in enumerate(owners):  # dark outline between different owners
        floor, col = divmod(idx, COLS)
        x, y = cell_xy(floor, col)
        if col < COLS - 1 and owners[idx + 1] != owner:
            fr.rect(x + CELL_W - 1, y, x + CELL_W - 1, y + CELL_H - 1, OUTLINE)
        if floor < FLOORS - 1 and owners[idx + COLS] != owner:
            fr.rect(x - (1 if col else 0), y - 1, x + CELL_W - 1, y - 1, OUTLINE)
    gx1 = GRID_X + COLS * CELL_W - 1
    fr.rect(GRID_X - 1, GRID_Y - 1, gx1, GRID_Y - 1, OUTLINE)
    fr.rect(GRID_X - 1, bottom, gx1, bottom, OUTLINE)
    fr.rect(GRID_X - 1, GRID_Y - 1, GRID_X - 1, bottom, OUTLINE)
    fr.rect(gx1, GRID_Y - 1, gx1, bottom, OUTLINE)

    for ident, (floor, c0, cells, label, arrow) in strips.items():
        x, y = cell_xy(floor, c0)
        fr.text(x + 3, y + 1, label, TEXT, shadow=OUTLINE)
        if arrow:
            fr.text(x + 3 + len(label) * ADV + 2, y + 1, arrow, ALERT, shadow=OUTLINE)
    if floor_segments(owners, "staff"):
        x, y = cell_xy(0, 0)
        fr.rect(x + 1, y + 1, x + 34, y + 8, STAFF_WALL)
        fr.text(x + 3, y + 1, "STAFF", TEXT_DIM, shadow=OUTLINE)
    vacant = floor_segments(owners, "vacant")
    if vacant:
        floor, c0, c1 = max(vacant, key=lambda s: (s[2] - s[1], s[0]))
        if c1 - c0 + 1 >= 6:
            x, y = cell_xy(floor, c0)
            cx = x + ((c1 - c0 + 1) * CELL_W - 8 * ADV) // 2
            fr.rect(cx - 3, y + 1, cx + 8 * ADV + 1, y + 9, VACANT_WALL)
            fr.text(cx, y + 2, "FOR RENT", TEXT_DIM)

    # status lines and the two clickable labels
    capacity, vacant_b = building["capacity_bytes"], building["vacant_bytes"]
    occupancy = ((capacity - vacant_b) * 100 + capacity // 2) // capacity
    fr.text(12, 219, f"OCCUPANCY {occupancy}%  {gb(vacant_b)} GB VACANT", TEXT_DIM)
    fr.text_right(376, 219, "F2 RENT ROLL  F3 SETTINGS", TEXT_DIM)
    fr.text(12, 230, building["remark"], TEXT)

    # rent roll
    lx0, lx1 = 224, 377
    fr.rect(lx0, 10, lx1, 213, PANEL_LINE)
    fr.rect(lx0 + 1, 11, lx1 - 1, 212, PANEL)
    fr.text(lx0 + 5, 15, "RENT ROLL", GOLD)
    as_of = price["as_of"]
    fr.text(lx0 + 5, 26, f"{building['memory_type']} {price_text(price)}/GB  {MONTHS[int(as_of[5:7]) - 1]} {int(as_of[8:10])}", TEXT_DIM)
    fr.rect(lx0 + 4, 36, lx1 - 4, 36, PANEL_LINE)
    fr.text(lx0 + 13, 40, "TENANT", TEXT_DIM)
    fr.text_right(lx0 + 121, 40, "GB", TEXT_DIM)
    fr.text_right(lx1 - 5, 40, "RENT", TEXT_DIM)
    roll, rest = roll_rows(building)
    y, owed = 51, 0
    for t in roll:
        fr.rect(lx0 + 5, y, lx0 + 9, y + 6, HUES[hue[t["id"]]][0])
        fr.text(lx0 + 13, y, t["name"][:8], TEXT)
        tag = tag_of(t)
        if tag:
            fr.text(lx0 + 66, y, tag, {"LEAK": ALERT, "SQUAT": GOLD}.get(tag, TEXT_DIM))
        fr.text_right(lx0 + 121, y, gb(t["rent_bytes"]), TEXT_DIM)
        fr.text_right(lx1 - 5, y, money(t["rent_bytes"], price), TEXT)
        owed += whole_units(t["rent_bytes"], price)
        y += 10
    if rest:
        rest_bytes = sum(t["rent_bytes"] for t in rest)
        fr.rect(lx0 + 5, y, lx0 + 9, y + 6, OTHERS_WALL)
        fr.text(lx0 + 13, y, f"OTHERS ({len(rest)})", TEXT_DIM)
        fr.text_right(lx0 + 121, y, gb(rest_bytes), TEXT_DIM)
        fr.text_right(lx1 - 5, y, money(rest_bytes, price), TEXT_DIM)
        owed += whole_units(rest_bytes, price)
        y += 10
    y += 2
    fr.rect(lx0 + 4, y, lx1 - 4, y, PANEL_LINE)
    y += 5
    fr.text(lx0 + 5, y, "RENT OWED", GOLD)
    fr.text_right(lx1 - 5, y, f"{price['symbol']}{owed}", GOLD)
    y += 12
    for swatch, label, nbytes, color in ((STAFF_WALL, "STAFF", building["staff_bytes"], TEXT_DIM),
                                         (VACANT_WINDOW, "VACANT", vacant_b, GOOD)):
        fr.rect(lx0 + 5, y, lx0 + 9, y + 6, swatch)
        fr.text(lx0 + 13, y, label, TEXT_DIM)
        fr.text_right(lx0 + 121, y, gb(nbytes), TEXT_DIM)
        fr.text_right(lx1 - 5, y, money(nbytes, price), color)
        y += 10
    y += 4
    own = building["own_bytes"]
    fr.text(lx0 + 5, y, f"MY OFFICE {own // MIB} MB", TEXT_DIM)
    fr.text_right(lx1 - 5, y, money(own, price, cents=True), TEXT_DIM)
    return fr.img, fill_order, owed


def render_card(building):
    """The rent roll card: 240x300, a paper receipt on the night sky."""
    price = building["price"]
    cw, ch, x0, x1 = 240, 300, 22, 217
    year, month, day = (int(p) for p in building["date"].split("-"))
    offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4]  # Sakamoto's day-of-week method
    yy = year - (month < 3)
    weekday = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"][
        (yy + yy // 4 - yy // 100 + yy // 400 + offsets[month - 1] + day) % 7]
    capacity_gb = (building["capacity_bytes"] + GIB // 2) // GIB

    lines = [("big", "RAMLORD"), ("center", "RENT ROLL"), ("center", f"{weekday} {day} {MONTHS[month - 1]} {year}"),
             ("center", f"{capacity_gb} GB {building['memory_type']} AT {price_text(price)}/GB"), ("rule", None)]
    roll, rest = roll_rows(building)
    owed = 0
    for t in roll:
        lines.append(("row", (t["name"], tag_of(t), gb(t["rent_bytes"]) + " GB", money(t["rent_bytes"], price))))
        owed += whole_units(t["rent_bytes"], price)
    if rest:
        rest_bytes = sum(t["rent_bytes"] for t in rest)
        lines.append(("row", (f"OTHERS ({len(rest)})", "", gb(rest_bytes) + " GB", money(rest_bytes, price))))
        owed += whole_units(rest_bytes, price)
    lines += [("rule", None), ("total", ("RENT OWED", f"{price['symbol']}{owed}")),
              ("row", ("VACANT", "", gb(building["vacant_bytes"]) + " GB", money(building["vacant_bytes"], price))),
              ("rule", None)]
    lines += [("center", s) for s in wrap_balanced(building["remark"], 30)]
    lines += [("gap", None), ("center", "THEY LIVED RENT-FREE."), ("center", "NOT ANYMORE.")]

    height = {"big": 20, "center": 10, "rule": 8, "row": 10, "total": 19, "gap": 5}
    body = sum(height[kind] for kind, _ in lines)
    top = (ch - body - 22) // 2
    bottom = top + body + 16
    fr = Frame(cw, ch, SKY)
    for sx, sy in [(8, 12), (12, 280), (230, 20), (228, 120), (10, 150), (232, 268), (12, 66), (229, 200), (6, 230)]:
        fr.dot(sx, sy, STAR)
    fr.rect(x0 + 2, top + 2, x1 + 2, bottom + 2, PAPER_SHADOW)
    fr.rect(x0, top, x1, bottom, PAPER)
    for x in range(x0, x1 + 1):  # torn lower edge
        if ((x - x0) // 3) % 2 == 0:
            fr.rect(x, bottom - 1, x, bottom + 2, SKY)
    y = top + 8
    for kind, payload in lines:
        if kind == "big":
            fr.text_center(x0, x1, y, payload, INK, scale=2)
        elif kind == "center":
            fr.text_center(x0, x1, y, payload, INK)
        elif kind == "rule":
            for x in range(x0 + 6, x1 - 5, 4):
                fr.rect(x, y + 2, x + 1, y + 2, PAPER_FAINT)
        elif kind == "row":
            name, tag, size, rent = payload
            fr.text(x0 + 6, y, name[:10], INK)
            if tag:
                fr.text(x0 + 68, y, tag, PAPER_ALERT if tag == "LEAK" else PAPER_FAINT)
            fr.text_right(x1 - 40, y, size, PAPER_FAINT)
            fr.text_right(x1 - 6, y, rent, INK)
        elif kind == "total":
            label, amount = payload
            fr.text(x0 + 6, y + 1, label, INK, scale=2)
            fr.text_right(x1 - 6, y + 1, amount, INK, scale=2)
        y += height[kind]
    return fr.img, owed


if __name__ == "__main__":
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(out_dir, exist_ok=True)
    building = sample_building()
    with open(os.path.join(out_dir, "sample-building.json"), "w") as f:
        json.dump(building, f, indent=2)
        f.write("\n")
    tower, fill_order, owed = render_tower(building)
    tower.resize((W * 3, H * 3), Image.NEAREST).save(os.path.join(out_dir, "tower.png"))
    card, card_owed = render_card(building)
    card.resize((240 * 4, 300 * 4), Image.NEAREST).save(os.path.join(out_dir, "rent-roll.png"))
    names = {t["id"]: t["name"] for t in building["tenants"]}
    for owner, n in fill_order:
        print(f"{names.get(owner, owner):10s} {n:3d} units")
    print("rent owed", owed, "card", card_owed)
