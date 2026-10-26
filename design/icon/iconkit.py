"""Shared SVG helpers for the Icon Composer app icon (see zpull.py)."""

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

