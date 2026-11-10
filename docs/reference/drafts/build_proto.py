# Assemble the reference prototype: the docstring and imports from proto_head.py,
# the glyph table from the working draft tower_proto.py, and the rest from proto_body.py.
#
#     python3 build_proto.py [OUT.py]
#
# Run it in this folder. The result is byte for byte ../prototype.py.
import sys

src = open('tower_proto.py').read()
g0 = src.index('GLYPHS = {')
g1 = src.index('ADV = 6')
glyphs = src[g0:g1].rstrip() + '\n'
assert glyphs.count('": [') == 57, glyphs.count('": [')
body = open('proto_body.py').read()
head = open('proto_head.py').read()
out = sys.argv[1] if len(sys.argv) > 1 else 'prototype.py'
open(out, 'w').write(head + glyphs + body)
print("glyphs", glyphs.count('": ['), "->", out)
