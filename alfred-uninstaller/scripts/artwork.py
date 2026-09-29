#!/usr/bin/env python3
"""Render small antialiased workflow glyphs with Python's standard library."""
from pathlib import Path
import math
import struct
import zlib

ROOT = Path(__file__).resolve().parent.parent / "workflow"
SIZE, SCALE = 128, 3

def segment(x, y, ax, ay, bx, by, width):
    t = max(0, min(1, ((x-ax)*(bx-ax)+(y-ay)*(by-ay))/((bx-ax)**2+(by-ay)**2)))
    return math.hypot(x-ax-t*(bx-ax), y-ay-t*(by-ay)) <= width/2

def rounded(x, y, left, top, right, bottom, radius):
    dx = max(left+radius-x, 0, x-(right-radius))
    dy = max(top+radius-y, 0, y-(bottom-radius))
    return dx*dx+dy*dy <= radius*radius

def glyph(name, x, y):
    white = (246, 249, 252)
    if name in ("icon", "trash"):
        if segment(x,y,42,42,46,97,6) or segment(x,y,46,97,82,97,6) or segment(x,y,82,97,86,42,6): return white
        if segment(x,y,36,37,92,37,6) or segment(x,y,54,26,74,26,5): return white
        if segment(x,y,55,51,57,83,4) or segment(x,y,73,51,71,83,4): return white
    elif name in ("checked", "unchecked"):
        outer = rounded(x,y,29,29,99,99,15)
        inner = rounded(x,y,34,34,94,94,10)
        if outer and not inner: return white
        if name == "checked" and (segment(x,y,45,64,59,78,7) or segment(x,y,59,78,84,49,7)): return white
    elif name == "back":
        if segment(x,y,49,64,86,64,7) or segment(x,y,45,64,64,43,7) or segment(x,y,45,64,64,85,7): return white
    elif name == "warning":
        if segment(x,y,64,33,64,72,7) or math.hypot(x-64,y-90)<4: return white
    return None

def png(name, color):
    raw = bytearray()
    for py in range(SIZE):
        raw.append(0)
        for px in range(SIZE):
            channels = [0,0,0,0]
            for sy in range(SCALE):
                for sx in range(SCALE):
                    x,y = px+(sx+.5)/SCALE, py+(sy+.5)/SCALE
                    if rounded(x,y,5,5,123,123,27):
                        rgb = glyph(name,x,y) or color
                        for i,c in enumerate(rgb): channels[i] += c
                        channels[3] += 255
            alpha = channels[3]/(SCALE*SCALE)
            rgb = [round(c*255/channels[3]) if channels[3] else 0 for c in channels[:3]]
            raw.extend(rgb + [round(alpha)])
    def chunk(tag,data):
        return struct.pack(">I",len(data))+tag+data+struct.pack(">I",zlib.crc32(tag+data)&0xffffffff)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR",struct.pack(">IIBBBBB",SIZE,SIZE,8,6,0,0,0)) + chunk(b"IDAT",zlib.compress(bytes(raw),9)) + chunk(b"IEND",b"")

ROOT.mkdir(exist_ok=True)
for name, color in [("icon",(42,83,116)),("trash",(42,83,116)),("checked",(43,108,102)),("unchecked",(90,99,108)),("back",(78,88,106)),("warning",(154,104,29))]:
    (ROOT/(name+".png")).write_bytes(png(name,color))
