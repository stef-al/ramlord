# Reference material

Everything here was made while writing `SPEC.md`. None of it is product code. Where it disagrees with `SPEC.md`, `SPEC.md` wins.

| Path | What it is |
| --- | --- |
| `tower.png` | The sample building at 3 times scale. The first milestone aims for this look. |
| `rent-roll.png` | The rent roll card for the sample building at 4 times scale. |
| `prototype.py` | The Python script that drew both images: a worked example of the layout, palette, font and labels. |
| `drafts/` | The working files behind `prototype.py` and five earlier renders. Superseded. |
| `wine-probe/` | A C program that tries the Win32 calls RAMlord needs under Wine, with its report and screenshot. |

`python3 prototype.py OUT_DIR` needs Pillow. It writes `tower.png`, `rent-roll.png` and `sample-building.json`. They match the two images here and `fixtures/sample-building.json` byte for byte.
