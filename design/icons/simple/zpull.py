#!/usr/bin/env python3
"""Zipling's Z-pull icon, refined: a polished zipper pull with a Z cut through
it, hanging from a short closed zipper. Drawn as SVG layers in an Icon
Composer document so macOS supplies the plate shape and glass edge.

Usage: python3 zpull.py [variant ...]
Writes icons/zpull-<variant>.icon and out/zpull-<variant>.png
"""

import json
import os
import subprocess
import sys

from make import ICTOOL, Svg, color, rrect

HERE = os.path.dirname(os.path.abspath(__file__))
CX = 512

METALS = {
    # Polished metal: bright top, a darker reflection band across the middle, light again below.
    "silver": dict(stops=["#FFFFFF", "#E4E9F1", "#A7B2C8", "#D9DFEA", "#FFFFFF"], edge="#7C89A5",
                   wall="#5F6B87", tooth=("#FFFFFF", "#9FAAC2")),
    "gold": dict(stops=["#FFF8DC", "#F7D98A", "#C08A2E", "#EDC874", "#FFF1C4"], edge="#94661F",
                 wall="#6E4A12", tooth=("#FFF1BE", "#C38C2F")),
    "graphite": dict(stops=["#8C919E", "#5D626E", "#2F333C", "#4A4F5A", "#6E7380"], edge="#1C1F25",
                     wall="#111317", tooth=("#9AA0AD", "#3A3E47")),
}

VARIANTS = {
    "indigo": dict(plate=("#6385FF", "#2B2FC6"), tape=("#1A2378", "#2C3AAE"), metal="silver",
                   dark_plate=("#2A3480", "#0E1236")),
    "midnight": dict(plate=("#2C3568", "#0B0F25"), tape=("#080B1C", "#1A2149"), metal="gold",
                     dark_plate=("#1C2148", "#05070F")),
    "sunset": dict(plate=("#FFA061", "#EC4436"), tape=("#A8281F", "#D2402E"), metal="silver"),
    "mint": dict(plate=("#62EBCC", "#10A39C"), tape=("#086A65", "#0E8E88"), metal="silver"),
    "cloud": dict(plate=("#FFFFFF", "#E3E8F1"), tape=("#2149D8", "#3E6CF2"), metal="silver", z="enamel",
                  enamel=("#4F7BFF", "#2244D6")),
    "graphite": dict(plate=("#43454D", "#121317"), tape=("#09090B", "#1E1F24"), metal="silver"),
    "violet": dict(plate=("#B47BFF", "#6629E6"), tape=("#3F1699", "#5A2ACB"), metal="silver"),
    "ivory": dict(plate=("#FFF9EE", "#F1E4CF"), tape=("#1D2440", "#2E3863"), metal="gold", z="enamel",
                  enamel=("#34407A", "#141A38")),
    "swing": dict(plate=("#6385FF", "#2B2FC6"), tape=("#1A2378", "#2C3AAE"), metal="silver", swing=13),
    "forest": dict(plate=("#2A6B52", "#0B2C22"), tape=("#06201A", "#123D31"), metal="gold"),
}


def metal_grad(svg, m, y0, y1):
    stops = m["stops"]
    return svg.grad([(round(i / (len(stops) - 1), 3), c, 1) for i, c in enumerate(stops)], 0, y0, 0, y1)


def hgrad(svg, edge, mid, x0, x1):
    return svg.grad([(0, edge, 1), (0.5, mid, 1), (1, edge, 1)], x0, 0, x1, 0)


def zipper(v, m, modern=False, glass_teeth=False):
    """Tape and teeth. modern: bolder teeth, no stitching. glass_teeth: teeth drawn plain white
    for a Liquid Glass layer (returns tape and teeth separately)."""
    svg = Svg()
    x0, w = CX - 66, 132
    svg.shadow(rrect(x0, -80, w, 450, 30), dy=12, blur=18, opacity=0.35)
    svg.add(f'<rect x="{x0}" y="-80" width="{w}" height="450" rx="22" fill="{hgrad(svg, v["tape"][0], v["tape"][1], x0, x0 + w)}"/>')
    if not modern:
        for x in (x0 + 11, x0 + w - 11):
            svg.add(f'<path d="M{x},-60 V352" stroke="#FFFFFF" stroke-opacity="0.32" stroke-width="3.5" '
                    f'stroke-dasharray="11 9" stroke-linecap="round"/>')
    # Interlocked teeth: alternating from each side across the centre line.
    teeth = Svg() if glass_teeth else svg
    face = "#FFFFFF" if glass_teeth else hgrad(svg, m["tooth"][1], m["tooth"][0], CX - 44, CX + 44)
    step, th, tl, off0 = (20, 24, 82, 15) if modern else (15, 17, 74, 13)
    if glass_teeth:
        # Glass merges touching shapes, so leave a gap between rows.
        step, th, tl, off0 = 24, 15, 80, 15
    y, k = -30, 0
    while y < 340:
        off = off0 if k % 2 else -off0
        tx = CX - tl / 2 + off
        if not glass_teeth:
            teeth.add(f'<rect x="{tx}" y="{y + 3}" width="{tl}" height="{th}" rx="{th * 0.4}" fill="#000000" fill-opacity="0.28"/>')
        edge = "" if glass_teeth else f'stroke="{m["edge"]}" stroke-width="1.5"'
        teeth.add(f'<rect x="{tx}" y="{y}" width="{tl}" height="{th}" rx="{th * 0.4}" fill="{face}" {edge}/>')
        if not glass_teeth:
            teeth.add(f'<rect x="{tx + 9}" y="{y + 4}" width="{tl - 18}" height="{th * 0.22}" rx="2" fill="#FFFFFF" fill-opacity="0.55"/>')
        y += step
        k += 1
    return (svg, teeth) if glass_teeth else svg


def slider(m):
    svg = Svg()
    body = ("M478,296 H546 C559,296 566,303 568,314 L592,404 C596,421 587,434 570,436 "
            "Q512,445 454,436 C437,434 428,421 432,404 L456,314 C458,303 465,296 478,296 Z")
    svg.shadow(body, dy=14, blur=14, opacity=0.4)
    svg.add(f'<path d="{body}" fill="{metal_grad(svg, m, 296, 445)}" stroke="{m["edge"]}" stroke-width="3"/>')
    svg.add(f'<path d="M482,306 H542" stroke="#FFFFFF" stroke-opacity="0.8" stroke-width="7" stroke-linecap="round"/>')
    # The bridge the pull hangs from.
    bridge = rrect(CX - 30, 328, 60, 100, 26)
    svg.shadow(bridge, dy=6, blur=6, opacity=0.35)
    svg.add(f'<path d="{bridge}" fill="{metal_grad(svg, m, 328, 428)}" stroke="{m["edge"]}" stroke-width="2.5"/>')
    svg.add(f'<path d="M{CX - 14},338 H{CX + 14}" stroke="#FFFFFF" stroke-opacity="0.75" stroke-width="6" '
            f'stroke-linecap="round"/>')
    return svg


def tab_path(top, bottom, rt, rb):
    """A tapered paddle: narrow rounded top, wide rounded bottom, smooth sides."""
    ct, cb = top + rt, bottom - rb
    return (f"M{CX - rt},{ct} A{rt},{rt} 0 0 1 {CX + rt},{ct} "
            f"C{CX + rt + 4},{ct + (cb - ct) * 0.35} {CX + rb},{cb - (cb - ct) * 0.4} {CX + rb},{cb} "
            f"A{rb},{rb} 0 0 1 {CX - rb},{cb} "
            f"C{CX - rb},{cb - (cb - ct) * 0.4} {CX - rt - 4},{ct + (cb - ct) * 0.35} {CX - rt},{ct} Z")


def z_path(cx, cy, size):
    w, h = size, size * 1.04
    return f"M{cx - w / 2},{cy - h / 2} H{cx + w / 2} L{cx - w / 2},{cy + h / 2} H{cx + w / 2}"


def pull(v, m):
    svg = Svg()
    top, bottom, rt, rb = 392, 868, 64, 128
    outline = tab_path(top, bottom, rt, rb)
    slot = rrect(CX - 34, top + 26, 68, 30, 15)
    zc, zs = top + (bottom - top) * 0.62, 124
    zsw = zs * 0.21
    pivot = f"rotate({v.get('swing', 0)} {CX} {top + 40})"
    svg.shadow(outline, dy=30, blur=26, opacity=0.38, transform=pivot)
    svg.n += 1
    mask, clip = f"m{svg.n}", f"c{svg.n}"
    cut = (f'<path d="{z_path(CX, zc - 3, zs)}" fill="none" stroke="#000" stroke-width="{zsw}" '
           f'stroke-linecap="round" stroke-linejoin="round"/>') if v.get("z", "cut") == "cut" else ""
    svg.defs.append(f'<mask id="{mask}" maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024">'
                    f'<rect width="1024" height="1024" fill="#fff"/><path d="{slot}" fill="#000"/>{cut}</mask>')
    svg.defs.append(f'<clipPath id="{clip}"><path d="{outline}"/></clipPath>')
    body = [f'<path d="{outline}" fill="{metal_grad(svg, m, top, bottom)}"/>']
    # Bevel: light along the upper-left inner edge, shade along the lower-right.
    body.append(f'<g clip-path="url(#{clip})">'
                f'<path d="{outline}" transform="translate(3 6)" fill="none" stroke="#FFFFFF" stroke-opacity="0.7" stroke-width="16"/>'
                f'<path d="{outline}" transform="translate(-3 -6)" fill="none" stroke="{m["wall"]}" stroke-opacity="0.35" stroke-width="16"/>'
                f'<path d="M{CX - 150},{top + 120} L{CX + 40},{top - 40} L{CX + 110},{top + 30} L{CX - 80},{top + 190} Z" '
                f'fill="#FFFFFF" fill-opacity="0.22"/></g>')
    body.append(f'<path d="{outline}" fill="none" stroke="{m["edge"]}" stroke-width="3"/>')
    if v.get("z", "cut") == "cut":
        # The cut's inner wall shows below each edge, so the Z reads as cut through thick metal.
        body.append(f'<path d="{z_path(CX, zc, zs)}" fill="none" stroke="{m["wall"]}" stroke-width="{zsw + 7}" '
                    f'stroke-linecap="round" stroke-linejoin="round"/>')
        body.append(f'<path d="{z_path(CX, zc - 1, zs)}" fill="none" stroke="{m["edge"]}" stroke-width="{zsw + 3}" '
                    f'stroke-linecap="round" stroke-linejoin="round"/>')
    else:
        # Enamel inlay: a glossy coloured Z set into a thin metal rim.
        e0, e1 = v["enamel"]
        zd = z_path(CX, zc, zs)
        round_ = 'stroke-linecap="round" stroke-linejoin="round"'
        body.append(f'<path d="{zd}" fill="none" stroke="{m["edge"]}" stroke-width="{zsw + 6}" {round_}/>')
        body.append(f'<path d="{zd}" fill="none" stroke="{svg.vgrad(e0, e1, zc - zs / 2, zc + zs / 2)}" '
                    f'stroke-width="{zsw}" {round_}/>')
        body.append(f'<path d="{zd}" transform="translate(0 -4)" fill="none" stroke="#FFFFFF" stroke-opacity="0.22" '
                    f'stroke-width="{zsw * 0.22}" {round_}/>')
    # Slot wall at the top, where the bridge passes through.
    body.append(f'<path d="{rrect(CX - 34, top + 27, 68, 30, 15)}" fill="none" stroke="{m["wall"]}" stroke-width="6"/>')
    svg.add(f'<g transform="{pivot}"><g mask="url(#{mask})">{"".join(body)}</g></g>')
    return svg


def glass_slider():
    svg = Svg()
    body = ("M478,296 H546 C559,296 566,303 568,314 L592,404 C596,421 587,434 570,436 "
            "Q512,445 454,436 C437,434 428,421 432,404 L456,314 C458,303 465,296 478,296 Z")
    svg.add(f'<path d="{body}" fill="#FFFFFF"/>')
    return svg


def glass_pull(v):
    svg = Svg()
    top, bottom, rt, rb = 392, 868, 64, 128
    outline = tab_path(top, bottom, rt, rb)
    slot = rrect(CX - 34, top + 26, 68, 30, 15)
    zc, zs = top + (bottom - top) * 0.62, 124
    svg.n += 1
    mask = f"m{svg.n}"
    svg.defs.append(f'<mask id="{mask}" maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024">'
                    f'<rect width="1024" height="1024" fill="#fff"/><path d="{slot}" fill="#000"/>'
                    f'<path d="{z_path(CX, zc, zs)}" fill="none" stroke="#000" stroke-width="{zs * 0.21}" '
                    f'stroke-linecap="round" stroke-linejoin="round"/></mask>')
    pivot = f"rotate({v.get('swing', 0)} {CX} {top + 40})"
    svg.add(f'<g transform="{pivot}"><path d="{outline}" fill="#FFFFFF" mask="url(#{mask})"/></g>')
    return svg


def backlight():
    """A soft pool of light on the plate behind the pull."""
    svg = Svg()
    svg.add(f'<rect width="1024" height="1024" fill="{svg.radial(CX, 470, 560, "#FFFFFF", 0.2)}"/>')
    return svg


GLASS = {"shadow": {"kind": "neutral", "opacity": 0.5}, "translucency": {"enabled": True, "value": 0.35},
         "specular": True, "blur-material": 0.3}
FLAT = {"shadow": {"kind": "none", "opacity": 0}, "specular": True}


def build(name, style="classic"):
    """style: classic (detailed metal), metal (modern metal), glasstab (Liquid Glass pull), glass (all glass)."""
    v = VARIANTS[name]
    m = METALS[v["metal"]]
    if style == "classic":
        layers = [("pull", pull(v, m), FLAT, False), ("slider", slider(m), FLAT, False),
                  ("zipper", zipper(v, m), FLAT, False)]
    elif style == "metal":
        layers = [("pull", pull(v, m), FLAT, False), ("slider", slider(m), FLAT, False),
                  ("zipper", zipper(v, m, modern=True), FLAT, False)]
    elif style == "glasstab":
        layers = [("pull", glass_pull(v), GLASS, True), ("slider", slider(m), FLAT, False),
                  ("zipper", zipper(v, m, modern=True), FLAT, False)]
    else:
        tape, teeth = zipper(v, m, modern=True, glass_teeth=True)
        layers = [("pull", glass_pull(v), GLASS, True), ("slider", glass_slider(), GLASS, True),
                  ("teeth", teeth, dict(GLASS, shadow={"kind": "neutral", "opacity": 0.3}), True),
                  ("tape", tape, FLAT, False)]
    layers.append(("light", backlight(), FLAT, False))
    tag = name if style == "classic" else f"{name}-{style}"
    bundle = os.path.join(HERE, "icons", f"zpull-{tag}.icon")
    os.makedirs(os.path.join(bundle, "Assets"), exist_ok=True)
    groups = []
    for lname, svg, group, is_glass in layers:
        with open(os.path.join(bundle, "Assets", f"{lname}.svg"), "w") as f:
            f.write(svg.text())
        groups.append(dict(group, layers=[{"image-name": f"{lname}.svg", "name": lname, "glass": is_glass}]))
    def plate(c):
        return {"linear-gradient": [color(c[0]), color(c[1])],
                "orientation": {"start": {"x": 0.5, "y": 0.0}, "stop": {"x": 0.5, "y": 1.0}}}

    manifest = {
        "fill": plate(v["plate"]),
        "groups": groups,
        "supported-platforms": {"squares": ["macOS"]},
    }
    if "dark_plate" in v:
        # A deeper plate in Dark mode, so glass and metal keep their contrast.
        del manifest["fill"]
        manifest["fill-specializations"] = [{"value": plate(v["plate"])},
                                            {"appearance": "dark", "value": plate(v["dark_plate"])}]
    with open(os.path.join(bundle, "icon.json"), "w") as f:
        json.dump(manifest, f, indent=2)
    out = os.path.join(HERE, "out", f"zpull-{tag}.png")
    subprocess.run([ICTOOL, bundle, "--export-image", "--output-file", out, "--platform", "macOS",
                    "--rendition", "Default", "--width", "1024", "--height", "1024", "--scale", "1"],
                   check=True, capture_output=True)
    print("built", tag)


if __name__ == "__main__":
    args = sys.argv[1:]
    style = "classic"
    if args[:1] == ["--style"]:
        style, args = args[1], args[2:]
    for n in args or VARIANTS:
        build(n, style)
