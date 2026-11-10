"""RAMlord reference prototype.

Draws the two reference images from the sample building in SPEC.md:

    python3 prototype.py OUT_DIR

writes OUT_DIR/sample-building.json, OUT_DIR/tower.png (384x240 at 3x) and
OUT_DIR/rent-roll.png (240x300 at 4x). Needs Pillow.

This is a worked example of the layout, palette, font and labels, not the
product. Where it disagrees with SPEC.md, SPEC.md wins. All sizes, money and
percentages use integer arithmetic so every platform gets the same pixels.
"""
import json
import os
import sys

from PIL import Image

# ------------------------------------------------------------ font: 5x7, advance 6
