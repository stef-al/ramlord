# Prototype drafts

These are the working files behind `../prototype.py`, kept for the record. Do not build from them: they show rules that `SPEC.md` later changed, and the first draft computes money in floating point, which `SPEC.md` forbids.

| File | What it is |
| --- | --- |
| `tower_proto.py` | The first working draft. `python3 tower_proto.py TOWER.png CARD.png` draws `tower_ref.png` and `rent_roll_ref.png`. |
| `proto_head.py`, `proto_body.py` | The tidied prototype in two parts, without the font. |
| `build_proto.py` | Joins the two parts with the glyph table from `tower_proto.py`. Its output is byte for byte `../prototype.py`. |
| `tower_v1.png`, `tower_v2.png` | The first tower renders, at the first price basis of $18.09 per GB. |
| `receipt_v1.png` | The first rent roll card. |
| `tower_ref.png`, `rent_roll_ref.png` | Later renders at $14.34 per GB, before the F2 and F3 hints were added. |

What changed between these drafts and `../prototype.py`:

- The price basis went from $18.09 to $14.34 per GB.
- Sizes are written as `6.4` under a `GB` heading, not `6.4G`.
- Tenants with equal rent are ordered by name.
- Money, sizes and percentages use integer arithmetic.
- The leak arrow has a gap before it, and the first status line carries the two key hints.
