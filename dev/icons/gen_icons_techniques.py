"""Icons of the note-technique row (same style as gen_icons.py: 24x24, stroke 1.75, two shades). Usage: python gen_icons_techniques.py OUT_DIR"""
import os, sys

OUT = sys.argv[1]
P_, S_ = "#E8EAEE", "#9AA3B2"


def p(d, c=P_):
    return f'<path d="{d}" stroke="{c}"/>'


def s(d):
    return p(d, S_)


def o(cx, cy, r):
    return f'<circle cx="{cx}" cy="{cy}" r="{r}" stroke="{P_}"/>'


def dot(cx, cy, r=1.6):
    return f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{P_}" stroke="none"/>'


def r(x, y, w, h, rx=2, c=P_):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" stroke="{c}"/>'


ICONS = {
    "copy": r(8, 8, 11.5, 12.5) + s("M5 15.5V5.5A1.5 1.5 0 0 1 6.5 4H14.5"),
    "cut": o(7, 17.5, 2.5) + o(17, 17.5, 2.5) + p("M8.7 15.7 17 4M15.3 15.7 7 4"),
    "paste": r(5, 5, 14, 16, 2.5, S_) + s("M9 5V3.5h6V5") + p("M8.5 11h7M8.5 15h5"),
    "dead-note": p("M7 6l10 10M17 6 7 16") + s("M4 20.5h16"),
    "harmonic": p("M12 3.5l6 8.5-6 8.5-6-8.5Z") + s("M12 8v8M9 12h6"),
    "palm-mute": s("M4.5 9V5.5h15V9") + dot(9, 17, 2.6) + p("M11.6 17V10") + s("M15.5 17h4"),
    "vibrato": p("M3 12c1.4-5 2.6-5 4 0s2.6 5 4 0 2.6-5 4 0 2.6 5 4 0"),
    "let-ring": dot(6.5, 16, 2.4) + p("M8.9 16V6") + s("M12.5 16h2.5M17 16h2.5M12.5 20h7"),
}

os.makedirs(OUT, exist_ok=True)
for name, body in ICONS.items():
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" '
           f'stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{body}</svg>\n')
    open(os.path.join(OUT, name + ".svg"), "w").write(svg)
print(len(ICONS), "icons")
