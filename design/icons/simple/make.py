#!/usr/bin/env python3
"""Zipling icons in the "simple object, light depth" style: a few flat shapes
with soft gradients, layered in Icon Composer so macOS adds the depth.

Usage: python3 make.py [concept ...]
Writes icons/<concept>.icon and out/<concept>.png (rendered by Apple's ictool).
"""

import json
import math
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ICTOOL = "/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool"


def color(hex_):
    h = hex_.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))
    return f"srgb:{r:.5f},{g:.5f},{b:.5f},1.00000"


class Svg:
    """One layer: shapes plus the gradients they use."""

    def __init__(self):
        self.defs, self.body, self.n = [], [], 0

    def grad(self, stops, x1=0, y1=0, x2=0, y2=1024):
        self.n += 1
        gid = f"g{self.n}"
        s = "".join(f'<stop offset="{o}" stop-color="{c}" stop-opacity="{a}"/>' for o, c, a in stops)
        self.defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" '
                         f'x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">{s}</linearGradient>')
        return f"url(#{gid})"

    def vgrad(self, top, bottom, y1, y2):
        return self.grad([(0, top, 1), (1, bottom, 1)], 0, y1, 0, y2)

    def radial(self, cx, cy, r, c, a0=1.0):
        self.n += 1
        gid = f"g{self.n}"
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="{cx}" cy="{cy}" r="{r}">'
                         f'<stop offset="0" stop-color="{c}" stop-opacity="{a0}"/>'
                         f'<stop offset="1" stop-color="{c}" stop-opacity="0"/></radialGradient>')
        return f"url(#{gid})"

    def add(self, el):
        self.body.append(el)

    def shadow(self, d, dy=26, blur=26, opacity=0.28, transform=""):
        """Soft drop shadow of a shape, drawn beneath what follows."""
        self.n += 1
        fid = f"f{self.n}"
        self.defs.append(f'<filter id="{fid}" x="-50%" y="-50%" width="200%" height="200%">'
                         f'<feGaussianBlur stdDeviation="{blur}"/></filter>')
        t = f' transform="{transform}"' if transform else ""
        self.add(f'<g{t}><path d="{d}" transform="translate(0 {dy})" fill="#1B2440" fill-opacity="{opacity}" '
                 f'filter="url(#{fid})"/></g>')

    def text(self):
        return (f'<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
                f'<defs>{"".join(self.defs)}</defs>{"".join(self.body)}</svg>')


def rrect(x, y, w, h, r):
    r = min(r, w / 2, h / 2)
    return (f"M{x + r},{y} H{x + w - r} A{r},{r} 0 0 1 {x + w},{y + r} V{y + h - r} "
            f"A{r},{r} 0 0 1 {x + w - r},{y + h} H{x + r} A{r},{r} 0 0 1 {x},{y + h - r} "
            f"V{y + r} A{r},{r} 0 0 1 {x + r},{y} Z")


def pill_tab(svg, cx, top, w, h, angle, fill, edge, hole=True):
    """A zipper pull: a rounded tab with a slot, hanging from (cx, top), turned by angle."""
    slot = rrect(cx - w * 0.17, top + h * 0.56, w * 0.34, h * 0.3, w * 0.17) if hole else ""
    svg.shadow(rrect(cx - w / 2, top, w, h, w / 2), dy=10, blur=9, opacity=0.3,
               transform=f"rotate({angle} {cx} {top})")
    svg.add(f'<g transform="rotate({angle} {cx} {top})">'
            f'<path d="{rrect(cx - w / 2, top, w, h, w / 2)} {slot}" fill="{fill}" fill-rule="evenodd" '
            f'stroke="{edge}" stroke-width="3"/>'
            f'<path d="{rrect(cx - w / 2 + 7, top + 8, w - 14, h * 0.4, (w - 14) / 2)}" fill="#FFFFFF" fill-opacity="0.35"/>'
            f'</g>')


def slider(svg, cx, cy, w, h, fill, edge, angle=0):
    """Slider body: wider at the bottom (open side), with the lug on top."""
    top_w = w * 0.72
    pts = [(cx - top_w / 2, cy - h / 2), (cx + top_w / 2, cy - h / 2), (cx + w / 2, cy + h / 2), (cx - w / 2, cy + h / 2)]
    d = "M" + " L".join(f"{x},{y}" for x, y in pts) + " Z"
    svg.add(f'<g transform="rotate({angle} {cx} {cy})">'
            f'<path d="{d}" fill="{fill}" stroke="{fill}" stroke-width="{h * 0.36}" stroke-linejoin="round"/>'
            f'<path d="{d}" fill="none" stroke="{edge}" stroke-width="3" stroke-linejoin="round" opacity="0.5"/>'
            f'<rect x="{cx - w * 0.16}" y="{cy - h * 0.15}" width="{w * 0.32}" height="{h * 0.5}" rx="{w * 0.1}" '
            f'fill="#FFFFFF" fill-opacity="0.35"/>'
            f'</g>')


def bezier(p0, p1, p2, p3, n=400):
    pts = []
    for i in range(n + 1):
        t = i / n
        mt = 1 - t
        pts.append((mt ** 3 * p0[0] + 3 * mt * mt * t * p1[0] + 3 * mt * t * t * p2[0] + t ** 3 * p3[0],
                    mt ** 3 * p0[1] + 3 * mt * mt * t * p1[1] + 3 * mt * t * t * p2[1] + t ** 3 * p3[1]))
    return pts


def resample(pts, step):
    out, acc = [pts[0]], 0.0
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        seg = math.hypot(x1 - x0, y1 - y0)
        while acc + seg >= step:
            t = (step - acc) / seg
            x0, y0 = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
            seg = math.hypot(x1 - x0, y1 - y0)
            out.append((x0, y0))
            acc = 0.0
        acc += seg
    return out


def teeth(svg, pts, pitch, tw, th, fill, edge, closed=True, side=1):
    """Zipper teeth along a path. Closed: alternating from each side across the centre line.
    Open: one row, all on `side` of the path."""
    samples = resample(pts, pitch / 2 if closed else pitch)
    for k, (x, y) in enumerate(samples):
        j = min(k, len(samples) - 2)
        (ax, ay), (bx, by) = samples[j], samples[j + 1]
        ang = math.degrees(math.atan2(by - ay, bx - ax))
        off = (th * 0.22 * (1 if k % 2 else -1)) if closed else th * 0.5 * side
        svg.add(f'<g transform="translate({x} {y}) rotate({ang})">'
                f'<rect x="{-tw / 2}" y="{off - th / 2}" width="{tw}" height="{th}" rx="{min(tw, th) * 0.32}" '
                f'fill="{fill}" stroke="{edge}" stroke-width="1.5"/></g>')


def tape(svg, pts, width, fill):
    d = "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in pts)
    svg.add(f'<path d="{d}" fill="none" stroke="{fill}" stroke-width="{width}" stroke-linecap="round" '
            f'stroke-linejoin="round"/>')


# ------------------------------------------------------------- concepts
# Each returns (plate top, plate bottom, [groups top-first]); a group is (shadow opacity, [layer svgs]).

FOLDERS = {
    "blue": dict(back=("#3F97F7", "#1C66D8"), front=("#86C8FF", "#3A8BF6"), edge="#2462C9", tape="#2D72DC"),
    "manila": dict(back=("#F6B940", "#DD9418"), front=("#FFDD80", "#F4B13A"), edge="#C9851A", tape="#D4901F"),
}


def folder(colorway="blue"):
    c = FOLDERS[colorway]
    back, papers, front, zip_, pull = Svg(), Svg(), Svg(), Svg(), Svg()
    back.shadow("M252,236 H772 A56,56 0 0 1 828,292 V736 A56,56 0 0 1 772,792 H252 A56,56 0 0 1 196,736 V292 "
                "A56,56 0 0 1 252,236 Z", dy=30, blur=30, opacity=0.3)
    back.add(f'<path d="M252,236 H404 C432,236 448,248 462,268 L482,296 H772 A56,56 0 0 1 828,352 V736 '
             f'A56,56 0 0 1 772,792 H252 A56,56 0 0 1 196,736 V292 A56,56 0 0 1 252,236 Z" '
             f'fill="{back.vgrad(*c["back"], 236, 792)}"/>')
    for x, y, rot, top in ((262, 282, -4, "#FFFFFF"), (300, 300, 3, "#F3F7FD")):
        papers.add(f'<path d="{rrect(x, y, 470, 330, 26)}" transform="rotate({rot} {x + 235} {y + 165})" '
                   f'fill="{papers.vgrad(top, "#DDE5F1", y, y + 330)}"/>')
    papers.add(f'<g fill="#C9D4E5">' + "".join(
        f'<rect x="{350 + 12 * k}" y="{342 + 34 * k}" width="{300 - 60 * k}" height="14" rx="7" transform="rotate(3 535 465)"/>'
        for k in range(2)) + '</g>')
    front.shadow(rrect(196, 384, 632, 408, 56), dy=6, blur=10, opacity=0.25)
    front.add(f'<path d="{rrect(196, 392, 632, 408, 56)}" fill="{c["edge"]}"/>')
    front.add(f'<path d="{rrect(196, 384, 632, 408, 56)}" fill="{front.vgrad(*c["front"], 384, 792)}"/>')
    front.add(f'<path d="{rrect(200, 386, 624, 60, 54)}" fill="#FFFFFF" fill-opacity="0.22"/>')
    track = [(262, 486), (762, 486)]
    tape(zip_, track, 46, c["tape"])
    teeth(zip_, track, 26, 13, 34, zip_.vgrad("#FFFFFF", "#D3DDEB", 466, 506), "#A9B8CF")
    metal = pull.vgrad("#FFFFFF", "#C7D2E3", 440, 700)
    slider(pull, 612, 486, 68, 52, metal, "#93A3BD", angle=-90)
    pill_tab(pull, 612, 492, 62, 178, 10, pull.vgrad("#FFFFFF", "#CCD6E6", 492, 680), "#9AAAC3")
    return "#FFFFFF", "#E9ECF2", [(0.45, [pull]), (0.15, [zip_]), (0.4, [front]), (0.25, [papers]), (0.5, [back])]


def pouch():
    body, zip_, pull = Svg(), Svg(), Svg()
    shape = ("M214,430 C214,368 258,346 312,344 H712 C766,346 810,368 810,430 V636 "
             "C810,740 736,790 636,794 H388 C288,790 214,740 214,636 Z")
    body.shadow(shape, dy=30, blur=30, opacity=0.3)
    body.add(f'<path d="{shape}" transform="translate(0 8)" fill="#C9381F"/>')
    body.add(f'<path d="{shape}" fill="{body.vgrad("#FF9563", "#EE4F36", 344, 794)}"/>')
    body.add(f'<path d="{shape}" fill="{body.radial(372, 470, 330, "#FFFFFF", 0.32)}"/>')
    body.add(f'<path d="M262,452 V630 C262,712 318,748 400,750 H624 C706,748 762,712 762,630 V452" fill="none" '
             f'stroke="#FFD9C9" stroke-opacity="0.75" stroke-width="6" stroke-dasharray="16 12" stroke-linecap="round"/>')
    track = bezier((262, 392), (420, 380), (604, 380), (762, 392))
    tape(zip_, track, 50, "#C93E28")
    teeth(zip_, track, 26, 13, 36, zip_.vgrad("#FFE7A0", "#DDA43A", 370, 410), "#B98428")
    gold = pull.vgrad("#FFE9A8", "#D9A03A", 350, 640)
    slider(pull, 640, 386, 70, 54, gold, "#A9782A", angle=-92)
    pill_tab(pull, 640, 394, 64, 186, -14, pull.vgrad("#FFE7A0", "#D59A35", 394, 590), "#A9782A")
    return "#FFF8EE", "#F4E6D2", [(0.45, [pull]), (0.15, [zip_]), (0.5, [body])]


def goldpull():
    zip_, pull = Svg(), Svg()
    gold_t = zip_.vgrad("#FFE6A3", "#D7A043", 0, 1024)
    rot = "rotate(-28 512 512)"
    closed = [(512, -120), (512, 470)]
    left = [(500, 560), (360, 1150)]
    right = [(524, 560), (664, 1150)]
    inner = Svg()
    tape(inner, closed, 92, "#33437A")
    tape(inner, [(492, 470)] + left, 52, "#33437A")
    tape(inner, [(532, 470)] + right, 52, "#33437A")
    teeth(inner, closed, 30, 15, 66, gold_t, "#9C6F25")
    teeth(inner, resample(left, 4), 30, 15, 46, gold_t, "#9C6F25", closed=False, side=-1)
    teeth(inner, resample(right, 4), 30, 15, 46, gold_t, "#9C6F25", closed=False, side=1)
    zip_.defs += inner.defs
    zip_.shadow("M466,-200 H558 V470 L700,1200 H324 L466,470 Z", dy=24, blur=24, opacity=0.35, transform=rot)
    zip_.add(f'<g transform="{rot}">{"".join(inner.body)}</g>')
    tab = Svg()
    gold = tab.vgrad("#FFEBB0", "#D2963A", 380, 760)
    slider(tab, 512, 508, 118, 92, gold, "#9C6F25", angle=0)
    pill_tab(tab, 512, 516, 120, 300, 0, tab.vgrad("#FFE7A6", "#CF9136", 516, 820), "#9C6F25")
    pull.defs += tab.defs
    pull.add(f'<g transform="{rot}">{"".join(tab.body)}</g>')
    return "#2E3D70", "#121933", [(0.5, [pull]), (0.3, [zip_])]


def box():
    body, lid, zip_, pull = Svg(), Svg(), Svg(), Svg()
    body.shadow(rrect(236, 330, 552, 470, 44), dy=30, blur=30, opacity=0.3)
    body.add(f'<path d="{rrect(236, 430, 552, 370, 44)}" fill="{body.vgrad("#FBB868", "#E5863A", 430, 800)}"/>')
    body.add(f'<path d="{rrect(372, 560, 280, 150, 22)}" fill="#FFF7EC"/>')
    body.add('<g fill="#E5C9A6">' + "".join(
        f'<rect x="402" y="{592 + 32 * k}" width="{220 - 50 * k}" height="14" rx="7"/>' for k in range(3)) + '</g>')
    lid.shadow(rrect(212, 318, 600, 136, 44), dy=12, blur=12, opacity=0.3)
    lid.add(f'<path d="{rrect(212, 326, 600, 136, 44)}" fill="#D9792F"/>')
    lid.add(f'<path d="{rrect(212, 318, 600, 136, 44)}" fill="{lid.vgrad("#FFCF8C", "#F29A4A", 318, 454)}"/>')
    lid.add(f'<path d="{rrect(220, 322, 584, 40, 20)}" fill="#FFFFFF" fill-opacity="0.3"/>')
    track = [(262, 454), (762, 454)]
    tape(zip_, track, 44, "#C9702E")
    teeth(zip_, track, 26, 13, 32, zip_.vgrad("#FFFFFF", "#E9DCCB", 438, 472), "#C2A584")
    cream = pull.vgrad("#FFFFFF", "#E7D8C4", 420, 680)
    slider(pull, 640, 454, 66, 50, cream, "#B99A74", angle=-90)
    pill_tab(pull, 640, 460, 60, 170, 8, pull.vgrad("#FFFFFF", "#E5D3BC", 460, 640), "#B99A74")
    return "#EEF6FF", "#D3E5FA", [(0.45, [pull]), (0.15, [zip_]), (0.35, [lid]), (0.5, [body])]


def page():
    paper, inside, zip_, pull = Svg(), Svg(), Svg(), Svg()
    outline = "M300,180 H604 L752,328 V804 A48,48 0 0 1 704,852 H320 A48,48 0 0 1 272,804 V228 A48,48 0 0 1 320,180 Z"
    paper.shadow(outline, dy=30, blur=30, opacity=0.35)
    paper.add(f'<path d="{outline}" fill="{paper.vgrad("#FFFFFF", "#E6ECF5", 180, 852)}"/>')
    paper.add(f'<path d="M604,180 V288 A40,40 0 0 0 644,328 H752 Z" fill="{paper.vgrad("#F4F7FC", "#CBD6E6", 180, 328)}"/>')
    inside.add(f'<path d="M512,560 L404,852 H620 Z" fill="{inside.vgrad("#FFB547", "#FF6A3D", 560, 852)}"/>')
    closed = [(512, 196), (512, 560)]
    tape(zip_, closed, 56, "#2F3B5C")
    tape(zip_, [(500, 566), (402, 852)], 34, "#2F3B5C")
    tape(zip_, [(524, 566), (622, 852)], 34, "#2F3B5C")
    silver = zip_.vgrad("#FFFFFF", "#C7D1E0", 0, 1024)
    teeth(zip_, closed, 26, 13, 40, silver, "#8E9AB0")
    teeth(zip_, resample([(500, 570), (406, 840)], 4), 26, 13, 30, silver, "#8E9AB0", closed=False, side=-1)
    teeth(zip_, resample([(524, 570), (618, 840)], 4), 26, 13, 30, silver, "#8E9AB0", closed=False, side=1)
    metal = pull.vgrad("#FFFFFF", "#C3CEDF", 520, 760)
    slider(pull, 512, 570, 70, 54, metal, "#8E9AB0")
    pill_tab(pull, 512, 576, 64, 180, -8, pull.vgrad("#FFFFFF", "#C9D3E3", 576, 760), "#8E9AB0")
    return "#5AAEFF", "#2262EE", [(0.45, [pull]), (0.15, [zip_]), (0.0, [inside]), (0.5, [paper])]


def pod():
    body, zip_, pull = Svg(), Svg(), Svg()
    shape = rrect(222, 252, 580, 540, 230)
    body.shadow(shape, dy=30, blur=30, opacity=0.3)
    body.add(f'<path d="{shape}" transform="translate(0 8)" fill="#4F2BC7"/>')
    body.add(f'<path d="{shape}" fill="{body.vgrad("#B096FF", "#6A41F0", 252, 792)}"/>')
    body.add(f'<path d="{rrect(252, 282, 520, 480, 202)}" fill="none" stroke="#E6DCFF" stroke-opacity="0.6" '
             f'stroke-width="6" stroke-dasharray="16 12" stroke-linecap="round"/>')
    body.add(f'<path d="{shape}" fill="{body.radial(400, 380, 320, "#FFFFFF", 0.35)}"/>')
    track = bezier((300, 470), (400, 560), (624, 560), (724, 470))
    tape(zip_, track, 48, "#5430D0")
    teeth(zip_, track, 26, 13, 34, zip_.vgrad("#FFE6A0", "#DBA33E", 440, 560), "#B4832C")
    gold = pull.vgrad("#FFEAA8", "#D69C3A", 470, 720)
    end = track[-60]
    slider(pull, end[0], end[1], 66, 50, gold, "#A9782A", angle=-55)
    pill_tab(pull, end[0], end[1] + 6, 60, 168, -18, pull.vgrad("#FFE7A0", "#D59A35", end[1], end[1] + 170), "#A9782A")
    return "#F6F2FF", "#E4DBFA", [(0.45, [pull]), (0.15, [zip_]), (0.5, [body])]


def rrect_points(x, y, w, h, r, n=600):
    """Points around a rounded rectangle, clockwise from the top-left straight."""
    pts = []
    corners = [(x + w - r, y + r, -90), (x + w - r, y + h - r, 0), (x + r, y + h - r, 90), (x + r, y + r, 180)]
    for cx, cy, a0 in corners:
        for k in range(25):
            a = math.radians(a0 + 90 * k / 24)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts + [pts[0]]


def suitcase():
    body, zip_, pull = Svg(), Svg(), Svg()
    outline = rrect(244, 300, 536, 470, 72)
    body.shadow(outline, dy=30, blur=30, opacity=0.3)
    body.add(f'<path d="M430,306 V246 A36,36 0 0 1 466,210 H558 A36,36 0 0 1 594,246 V306" fill="none" '
             f'stroke="{body.vgrad("#2A3550", "#1A2236", 210, 306)}" stroke-width="30" stroke-linecap="round"/>')
    for x in (330, 694):
        body.add(f'<circle cx="{x}" cy="786" r="22" fill="#1E2638"/>')
    body.add(f'<path d="{outline}" transform="translate(0 8)" fill="#0F7F7E"/>')
    body.add(f'<path d="{outline}" fill="{body.vgrad("#45DCCB", "#14A3A0", 300, 770)}"/>')
    body.add(f'<path d="{rrect(252, 306, 520, 120, 66)}" fill="#FFFFFF" fill-opacity="0.18"/>')
    for x in (380, 644):
        body.add(f'<rect x="{x - 9}" y="430" width="18" height="290" rx="9" fill="#0E8D8B" fill-opacity="0.45"/>')
    track = rrect_points(284, 340, 456, 390, 52)
    tape(zip_, track, 40, "#0E8A88")
    teeth(zip_, track, 24, 12, 30, zip_.vgrad("#FFFFFF", "#D7E6E5", 0, 1024), "#9DBDBB")
    metal = pull.vgrad("#FFFFFF", "#CADBDA", 300, 560)
    slider(pull, 646, 340, 62, 48, metal, "#8FB0AE", angle=-90)
    pill_tab(pull, 646, 346, 56, 160, 12, pull.vgrad("#FFFFFF", "#C8D9D8", 346, 510), "#8FB0AE")
    return "#FFF5E9", "#F7DFC6", [(0.45, [pull]), (0.15, [zip_]), (0.5, [body])]


def envelope():
    body, flap, zip_, pull = Svg(), Svg(), Svg(), Svg()
    outline = rrect(204, 316, 616, 420, 48)
    body.shadow(outline, dy=30, blur=30, opacity=0.3)
    body.add(f'<path d="{outline}" transform="translate(0 8)" fill="#C9D1DE"/>')
    body.add(f'<path d="{outline}" fill="{body.vgrad("#FFFFFF", "#E6EBF3", 316, 736)}"/>')
    body.add(f'<path d="M222,720 L512,500 L802,720" fill="none" stroke="#D3DAE6" stroke-width="8" stroke-linejoin="round"/>')
    flap.shadow("M226,330 L512,596 L798,330 Z", dy=10, blur=12, opacity=0.25)
    flap.add(f'<path d="M226,326 L512,592 L798,326 Z" fill="{flap.vgrad("#F7F9FC", "#DCE3EE", 326, 592)}" '
             f'stroke="{flap.vgrad("#F7F9FC", "#DCE3EE", 326, 592)}" stroke-width="40" stroke-linejoin="round"/>')
    left, right = [(262, 360), (500, 580)], [(524, 580), (762, 360)]
    for seg in (left, right):
        tape(zip_, seg, 40, "#F0623F")
        teeth(zip_, resample(seg, 3), 24, 12, 30, zip_.vgrad("#FFFFFF", "#F3E1DA", 300, 620), "#D9A497")
    metal = pull.vgrad("#FFFFFF", "#E2D3CD", 540, 790)
    slider(pull, 512, 594, 62, 48, metal, "#C29A8E")
    pill_tab(pull, 512, 600, 56, 164, -6, pull.vgrad("#FFFFFF", "#E6D2CA", 600, 770), "#C29A8E")
    return "#FF8F6B", "#EE4F3A", [(0.45, [pull]), (0.15, [zip_]), (0.3, [flap]), (0.5, [body])]


def engraved_z(svg, cx, cy, size, dark, light):
    """A Z struck into metal: a dark groove with a light lower lip."""
    w, h = size, size * 1.05
    d = f"M{cx - w / 2},{cy - h / 2} H{cx + w / 2} L{cx - w / 2},{cy + h / 2} H{cx + w / 2}"
    sw = size * 0.2
    svg.add(f'<path d="{d}" transform="translate(0 3)" fill="none" stroke="{light}" stroke-width="{sw}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>')
    svg.add(f'<path d="{d}" fill="none" stroke="{dark}" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round"/>')


def zpull():
    zip_, pull = Svg(), Svg()
    track = [(512, 128), (512, 400)]
    zip_.shadow(rrect(452, 110, 120, 320, 60), dy=20, blur=20, opacity=0.3)
    tape(zip_, track, 120, "#1F2B8F")
    teeth(zip_, track, 32, 16, 74, zip_.vgrad("#FFFFFF", "#C2CBDD", 100, 420), "#8C98B4")
    metal = pull.vgrad("#FFFFFF", "#B7C2D6", 380, 860)
    slider(pull, 512, 420, 150, 112, metal, "#7F8CA8")
    tab_top, tab_w, tab_h = 452, 210, 400
    outline = rrect(512 - tab_w / 2, tab_top, tab_w, tab_h, tab_w / 2)
    pull.shadow(outline, dy=26, blur=24, opacity=0.35)
    pull.add(f'<path d="{outline}" fill="{metal}" stroke="#8592AE" stroke-width="4"/>')
    pull.add(f'<path d="{rrect(512 - tab_w / 2 + 14, tab_top + 14, tab_w - 28, 150, (tab_w - 28) / 2)}" '
             f'fill="#FFFFFF" fill-opacity="0.45"/>')
    engraved_z(pull, 512, tab_top + tab_h * 0.6, 92, "#8C98B2", "#FFFFFF")
    return "#5B7CFF", "#2B2FC9", [(0.0, [pull]), (0.0, [zip_])]


def clip():
    stack, top, clip_ = Svg(), Svg(), Svg()
    sheets = ["#7DD3A8", "#7CB8FF", "#FF8E7A", "#FFD25E"]
    stack.shadow(rrect(232, 330, 560, 470, 40), dy=30, blur=30, opacity=0.3)
    for k, col in enumerate(sheets):
        y = 346 + (len(sheets) - k) * 22
        stack.add(f'<path d="{rrect(232, y, 560, 420, 40)}" fill="{stack.vgrad(col, col, y, y + 420)}"/>')
        stack.add(f'<path d="{rrect(232, y, 560, 420, 40)}" fill="#000000" fill-opacity="0.06" transform="translate(0 4)"/>')
    top.add(f'<path d="{rrect(232, 330, 560, 420, 40)}" fill="{top.vgrad("#FFFFFF", "#E9EEF5", 330, 750)}"/>')
    top.add('<g fill="#C9D3E2">' + "".join(
        f'<rect x="300" y="{470 + 46 * k}" width="{400 - 70 * k}" height="20" rx="10"/>' for k in range(4)) + '</g>')
    # Binder clip on the top edge: silver handles folded up, a black jaw.
    wire = clip_.vgrad("#FFFFFF", "#AEB8C9", 160, 360)
    for x0, lean in ((424, -7), (528, 7)):
        clip_.add(f'<path d="M{x0},336 V214 A36,36 0 0 1 {x0 + 72},214 V336" transform="rotate({lean} {x0 + 36} 336)" '
                  f'fill="none" stroke="{wire}" stroke-width="15" stroke-linecap="round"/>')
    jaw = "M392,312 H632 L664,420 H360 Z"
    clip_.shadow(jaw, dy=12, blur=12, opacity=0.35)
    clip_.add(f'<path d="{jaw}" fill="{clip_.vgrad("#4A4F5C", "#16181E", 312, 420)}" stroke="#16181E" '
              f'stroke-width="18" stroke-linejoin="round"/>')
    clip_.add('<path d="M404,326 H620" stroke="#FFFFFF" stroke-opacity="0.35" stroke-width="8" stroke-linecap="round"/>')
    return "#EAF4FF", "#CFE2F8", [(0.0, [clip_]), (0.0, [top]), (0.0, [stack])]


def cube():
    body, zip_, pull = Svg(), Svg(), Svg()
    F, R, B, L = (512, 470), (772, 320), (512, 170), (252, 320)
    H = 320

    def down(p, d):
        return (p[0], p[1] + d)

    def poly(pts):
        return "M" + " L".join(f"{x},{y}" for x, y in pts) + " Z"

    top = poly([F, R, B, L])
    left = poly([L, F, down(F, H), down(L, H)])
    right = poly([F, R, down(R, H), down(F, H)])
    body.shadow(poly([L, B, R, down(R, H), down(F, H), down(L, H)]), dy=30, blur=30, opacity=0.3)
    for d, (c0, c1) in ((left, ("#8E73FF", "#6B4BEF")), (right, ("#6E50F0", "#4C2DD6")), (top, ("#D3C6FF", "#A893FF"))):
        g = body.vgrad(c0, c1, 170, 790)
        body.add(f'<path d="{d}" fill="{g}" stroke="{g}" stroke-width="34" stroke-linejoin="round"/>')
    body.add(f'<path d="M{L[0] + 12},{L[1] + 4} L{F[0]},{F[1] - 6} L{R[0] - 12},{R[1] + 4}" fill="none" '
             f'stroke="#FFFFFF" stroke-opacity="0.5" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>')
    band = 120
    left_track = [down(L, band), down(F, band)]
    right_track = [down(F, band), down(R, band)]
    silver = zip_.vgrad("#FFFFFF", "#D5DAE8", 400, 700)
    for t in (left_track, right_track):
        tape(zip_, t, 44, "#3E22B8")
        teeth(zip_, resample(t, 3), 24, 12, 32, silver, "#9AA2C0")
    metal = pull.vgrad("#FFFFFF", "#C8CFE0", 560, 820)
    sx, sy = 640, 470 + band - 74
    slider(pull, sx, sy, 62, 48, metal, "#8E97B5", angle=-60)
    pill_tab(pull, sx, sy + 6, 58, 170, 4, pull.vgrad("#FFFFFF", "#C9D0E1", sy, sy + 170), "#8E97B5")
    return "#F3F0FF", "#DED6FA", [(0.0, [pull]), (0.0, [zip_]), (0.0, [body])]


CONCEPTS = {"folder": folder, "folder-manila": lambda: folder("manila"), "zpull": zpull, "clip": clip, "cube": cube, "pouch": pouch, "goldpull": goldpull, "box": box, "page": page, "pod": pod,
            "suitcase": suitcase, "envelope": envelope}


def build(name):
    top, bottom, groups = CONCEPTS[name]()
    bundle = os.path.join(HERE, "icons", f"{name}.icon")
    os.makedirs(os.path.join(bundle, "Assets"), exist_ok=True)
    manifest_groups = []
    for gi, (shadow, layers) in enumerate(groups):
        entries = []
        for li, svg in enumerate(layers):
            asset = f"g{gi}l{li}.svg"
            with open(os.path.join(bundle, "Assets", asset), "w") as f:
                f.write(svg.text())
            entries.append({"image-name": asset, "name": asset[:-4], "glass": False})
        manifest_groups.append({"layers": entries,
                                "shadow": {"kind": "neutral" if shadow else "none", "opacity": shadow},
                                "specular": True})
    manifest = {
        "fill": {"linear-gradient": [color(top), color(bottom)],
                 "orientation": {"start": {"x": 0.5, "y": 0.0}, "stop": {"x": 0.5, "y": 1.0}}},
        "groups": manifest_groups,
        "supported-platforms": {"squares": ["macOS"]},
    }
    with open(os.path.join(bundle, "icon.json"), "w") as f:
        json.dump(manifest, f, indent=2)
    os.makedirs(os.path.join(HERE, "out"), exist_ok=True)
    subprocess.run([ICTOOL, bundle, "--export-image", "--output-file", os.path.join(HERE, "out", f"{name}.png"),
                    "--platform", "macOS", "--rendition", "Default", "--width", "1024", "--height", "1024",
                    "--scale", "1"], check=True, capture_output=True)
    print("built", name)


if __name__ == "__main__":
    for n in sys.argv[1:] or CONCEPTS:
        build(n)
