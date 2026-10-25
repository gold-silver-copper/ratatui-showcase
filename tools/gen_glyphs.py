# Regenerates src/glyph_table.rs from foot's box-drawing.c:
#   python3 tools/gen_glyphs.py path/to/foot/box-drawing.c src/glyph_table.rs
import re, sys
src = open(sys.argv[1]).read().split('\n')
out = open(sys.argv[2], 'w')
# Octants
start = next(i for i,l in enumerate(src) if 'matrix[230]' in l)
body = []
for l in src[start+1:]:
    if l.strip().startswith('};'): break
    body.append(re.sub(r'/\*.*?\*/', '', l))
entries = [e.strip() for e in ' '.join(body).split(',') if e.strip()]
assert len(entries) == 230, len(entries)
bit = dict(UPPER_LEFT=0, MIDDLE_UP_LEFT=1, MIDDLE_DOWN_LEFT=2, LOWER_LEFT=3, UPPER_RIGHT=4, MIDDLE_UP_RIGHT=5, MIDDLE_DOWN_RIGHT=6, LOWER_RIGHT=7)
octs = []
for e in entries:
    v = 0
    for n in e.split('|'): v |= 1 << bit[n.strip()]
    octs.append(v)
# Wedges
fs = next(i for i,l in enumerate(src) if l.startswith('draw_wedge_triangle(struct buf'))
val = dict(width='1.0', height='1.0', halfs0='0.5', halfs1='0.5', thirds0='1.0 / 3.0', thirds1='2.0 / 3.0')
val['0'] = '0.0'; val['height / 2'] = '0.5'; val['width / 2'] = '0.5'
tris, pending, cur = {}, [], {}
for l in src[fs:]:
    m = re.match(r'\s*case (0x[0-9a-f]+):', l)
    if m: pending.append(int(m.group(1), 16)); continue
    if re.match(r'\s*break;', l):
        if pending:
            p = [(cur['p%d_x'%k], cur['p%d_y'%k]) for k in (1,2,3)]
            for c in pending: tris[c] = p
        pending, cur = [], {}
        continue
    if l.startswith('}'): break
    for stmt in l.split(';'):
        parts = [x.strip() for x in stmt.split('=')]
        if len(parts) >= 2 and all(re.fullmatch(r'p[123]_[xy]', n) for n in parts[:-1]):
            for name in parts[:-1]: cur[name] = cur[parts[-1]] if parts[-1] in cur else val[parts[-1]]
def cases(lines_from):
    s = set()
    for a,b in re.findall(r'case (0x[0-9a-f]+)(?: \.\.\. (0x[0-9a-f]+))?:', lines_from):
        lo = int(a,16); hi = int(b,16) if b else lo
        s.update(range(lo, hi+1))
    return s
disp = '\n'.join(src)
i = disp.index('case 0x1fb3c ... 0x1fb40:')
blk = disp[i:disp.index('case 0x1fb9a:', i)]
normal = cases(blk.split('draw_wedge_triangle(buf, wc);')[0])
rest = blk.split('draw_wedge_triangle(buf, wc);')[1]
inverted = cases(rest.split('draw_wedge_triangle_inverted(buf, wc);')[0])
boxed = cases(rest.split('draw_wedge_triangle_inverted(buf, wc);')[1])
# box: 1fb46/1fb51 lower two thirds... per foot: y1 = thirds1 -> bottom third; 1fb5c/1fb67 top third
out.write('//! Generated from foot\'s box-drawing.c (MIT) by tools/gen_glyphs.py: the shapes\n//! of octants and wedges, as foot draws them.\n\n')
out.write('/// `U+1CD00 + i`: bit 0..3 = left column top to bottom, 4..7 = right column.\n')
out.write('pub const OCTANTS: [u8; 230] = [%s];\n\n' % ', '.join(str(v) for v in octs))
out.write('#[derive(Clone, Copy)]\npub enum Fill {\n    Triangle,\n    Outside,\n    TriangleAndBottomThird,\n    TriangleAndTopThird,\n}\n\n')
out.write('/// Wedge glyphs: a triangle (cell-relative corners) and how it is filled.\npub type Wedge = (char, [(f32, f32); 3], Fill);\n\npub const WEDGES: &[Wedge] = &[\n')
for c in sorted(tris):
    if c in normal: f = 'Triangle'
    elif c in inverted: f = 'Outside'
    elif c in boxed: f = 'TriangleAndBottomThird' if c in (0x1fb46, 0x1fb51) else 'TriangleAndTopThird'
    else: continue
    p = ', '.join('(%s, %s)' % xy for xy in tris[c])
    out.write("    ('\\u{%x}', [%s], Fill::%s),\n" % (c, p, f))
out.write('];\n')
print(len(octs), 'octants,', sum(1 for c in tris if c in normal|inverted|boxed), 'wedges')
