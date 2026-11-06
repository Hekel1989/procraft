#!/usr/bin/env python3
"""Writes docs/generation/mockups/results-tray.html: static SVG scenes, no scripts."""
import sys

OUT = sys.argv[1]
ACCENT = "#0076FD"

DEFS = """<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>
<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1b1f5e"/><stop offset=".45" stop-color="#6b2f8a"/><stop offset=".72" stop-color="#e2668a"/><stop offset=".9" stop-color="#f6b261"/></linearGradient>
<linearGradient id="r1" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0f163f"/><stop offset=".5" stop-color="#4b2a7a"/><stop offset=".8" stop-color="#d9726e"/><stop offset="1" stop-color="#f4c47f"/></linearGradient>
<linearGradient id="r2" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0b2a3a"/><stop offset=".55" stop-color="#1f6f7a"/><stop offset="1" stop-color="#9fd6c2"/></linearGradient>
<linearGradient id="r3" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3a160e"/><stop offset=".5" stop-color="#b2472a"/><stop offset="1" stop-color="#f6c26b"/></linearGradient>
<linearGradient id="r4" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#14213d"/><stop offset=".6" stop-color="#3d5a80"/><stop offset="1" stop-color="#e0c3a3"/></linearGradient>
<g id="art-original">
  <rect width="1120" height="800" fill="url(#sky)"/>
  <circle cx="800" cy="420" r="62" fill="#fff1d2"/><circle cx="800" cy="420" r="120" fill="#ffd9a0" opacity=".18"/>
  <path d="M0 520 C220 440 400 500 600 460 S940 420 1120 470 V800 H0z" fill="#7c3c92"/>
  <path d="M0 620 C240 560 520 620 760 590 S1000 560 1120 590 V800 H0z" fill="#3f1f5c"/>
</g>
<g id="art-r1">
  <rect width="1120" height="800" fill="url(#r1)"/><circle cx="820" cy="250" r="70" fill="#fff4d6" opacity=".95"/>
  <path d="M0 560 C200 470 380 520 560 480 S900 440 1120 500 V800 H0z" fill="#3a1f5a"/>
  <path d="M0 640 C260 580 520 640 760 600 S1000 560 1120 600 V800 H0z" fill="#1d0f33"/>
  <path d="M520 600 l14 -60 14 60z M560 610 l18 -80 18 80z" fill="#0d0820"/>
  <rect x="610" y="640" width="26" height="110" fill="#e9e0d0"/><rect x="606" y="626" width="34" height="18" fill="#c0392b"/>
  <path d="M0 740 C300 700 700 760 1120 720 V800 H0z" fill="#120a24"/>
</g>
<g id="art-r2">
  <rect width="1120" height="800" fill="url(#r2)"/><circle cx="300" cy="230" r="58" fill="#f2fbf6" opacity=".9"/>
  <path d="M0 520 C260 430 520 520 760 470 S1000 430 1120 480 V800 H0z" fill="#16444f"/>
  <path d="M0 650 C300 590 620 660 1120 610 V800 H0z" fill="#0a2630"/>
</g>
<g id="art-r3">
  <rect width="1120" height="800" fill="url(#r3)"/><circle cx="560" cy="330" r="90" fill="#ffe3a3"/>
  <path d="M0 560 C240 500 480 560 720 520 S1000 500 1120 540 V800 H0z" fill="#6a1f12"/>
  <path d="M0 680 C320 630 700 690 1120 650 V800 H0z" fill="#2a0c07"/>
</g>
<g id="art-r4">
  <rect width="1120" height="800" fill="url(#r4)"/><circle cx="880" cy="200" r="48" fill="#fff" opacity=".85"/>
  <path d="M0 540 C260 480 560 540 820 500 S1040 480 1120 500 V800 H0z" fill="#293e5c"/>
  <path d="M0 660 C340 610 680 670 1120 630 V800 H0z" fill="#101c2e"/>
</g>
</defs></svg>"""

ARTS = ["art-r1", "art-r2", "art-r3", "art-r4"]
# a round arrow coming back on itself (mockup only; the app draws its own)
AGAIN = ("M17.65 6.35A7.958 7.958 0 0 0 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08"
         "A5.99 5.99 0 0 1 12 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z")


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def topbar(running=False, ai_lit=False):
    parts = ['<rect x="0" y="0" width="1120" height="44" fill="#1c1c1c" fill-opacity=".92"/>',
             '<text x="40" y="28" font-size="17" fill="#aaa">Gallery</text>']
    x = 120
    for i in range(5):
        lit = ai_lit and i == 4
        parts.append(f'<circle cx="{x}" cy="22" r="15" fill="{ACCENT if lit else "#141414"}"/>')
        if i == 4:
            parts.append(f'<text x="{x}" y="27" font-size="12" font-weight="700" text-anchor="middle" fill="{"#fff" if lit else "#c7c7c7"}">AI</text>')
            if running:
                parts.append(f'<circle cx="{x}" cy="22" r="18" fill="none" stroke="{ACCENT}" stroke-width="2.4" stroke-dasharray="70 113" stroke-linecap="round" transform="rotate(-90 {x} 22)"/>')
        else:
            parts.append(f'<rect x="{x-6}" y="16" width="12" height="12" rx="3" fill="#c7c7c7" opacity=".85"/>')
        x += 50
    for i, cx in enumerate([880, 930, 980, 1030]):
        parts.append(f'<rect x="{cx-10}" y="12" width="20" height="20" rx="5" fill="{ACCENT if i == 0 else "#d6d6d6"}" opacity="{1 if i == 0 else .8}"/>')
    parts.append('<circle cx="1072" cy="22" r="15" fill="#f4f0e6"/>')
    # side sliders
    parts.append('<rect x="0" y="140" width="44" height="460" rx="14" fill="#141414" fill-opacity=".85"/>')
    parts.append('<rect x="6" y="146" width="32" height="150" rx="8" fill="#3a3a3a"/><rect x="9" y="236" width="26" height="14" rx="7" fill="#c8c8c8"/>')
    parts.append('<rect x="12" y="316" width="20" height="20" rx="5" fill="none" stroke="#9a9a9a" stroke-width="2"/>')
    parts.append('<rect x="6" y="360" width="32" height="150" rx="8" fill="#3a3a3a"/><rect x="9" y="368" width="26" height="14" rx="7" fill="#c8c8c8"/>')
    return "\n".join(parts)


def thumb(x, y, art, on=False, size=66):
    s = [f'<svg x="{x}" y="{y}" width="{size}" height="{size}" viewBox="200 0 800 800" preserveAspectRatio="xMidYMid slice"><use href="#{art}"/></svg>',
         f'<rect x="{x}" y="{y}" width="{size}" height="{size}" rx="10" fill="none" stroke="#202020" stroke-width="4"/>']
    if on:
        s.append(f'<rect x="{x-2}" y="{y-2}" width="{size+4}" height="{size+4}" rx="11" fill="none" stroke="{ACCENT}" stroke-width="3"/>')
    return "\n".join(s)


def button(x, y, w, label, primary=False, h=40):
    fill = ACCENT if primary else "rgba(255,255,255,.09)"
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="{fill}"/>'
            f'<text x="{x + w / 2}" y="{y + h / 2 + 5}" font-size="14" font-weight="500" text-anchor="middle" fill="#fff">{esc(label)}</text>')


def tray(shown=1, close="tuck", menu=False, deleting=None, count=4):
    """The tray at the bottom: thumbnails, seed, Again, Apply, and its right-hand button."""
    left, right, top = 140, 980, 682
    s = [f'<rect x="{left}" y="{top}" width="{right-left}" height="96" rx="22" fill="#202020" fill-opacity=".94"/>']
    x = left + 16
    for i in range(count):
        s.append(thumb(x, top + 15, ARTS[i % 4], on=(i == shown)))
        if deleting == i:
            bx = x + 33
            s.append(f'<rect x="{bx-38}" y="{top-36}" width="76" height="32" rx="10" fill="#e84f51"/>'
                     f'<text x="{bx}" y="{top-15}" font-size="14" font-weight="500" text-anchor="middle" fill="#fff">Delete</text>')
        x += 76
    s.append(f'<rect x="{x}" y="{top+21}" width="1" height="54" fill="#fff" fill-opacity=".12"/>')
    # today's seed tag (the tray has it in ··· instead)
    if close != "tuck":
        s.append(f'<rect x="{x+16}" y="{top+30}" width="112" height="36" rx="10" fill="rgba(255,255,255,.06)"/>'
                 f'<text x="{x+30}" y="{top+53}" font-size="12.5" fill="#9a9a9a">seed 18234  ⧉</text>')
    bx = right - 16
    if close == "tuck":
        # a round button with a chevron pointing down: put the tray away
        cx = bx - 20
        s.append(f'<circle cx="{cx}" cy="{top+48}" r="20" fill="rgba(255,255,255,.09)"/>'
                 f'<path d="M{cx-7} {top+44} l7 7 l7 -7" stroke="#e6e6e6" stroke-width="2.4" fill="none" stroke-linecap="round" stroke-linejoin="round"/>')
        bx = cx - 30
        # the more button
        cx = bx - 20
        s.append(f'<circle cx="{cx}" cy="{top+48}" r="20" fill="rgba(255,255,255,.09)"/>'
                 + "".join(f'<circle cx="{cx + d}" cy="{top+48}" r="2.4" fill="#e6e6e6"/>' for d in (-7, 0, 7)))
        more_x = cx
        bx = cx - 30
    else:
        cx = bx - 14
        s.append(f'<text x="{cx}" y="{top+55}" font-size="20" text-anchor="middle" fill="#9a9a9a">✕</text>')
        bx = cx - 26
        more_x = None
    s.append(button(bx - 84, top + 28, 84, "Apply", primary=True))
    if close == "tuck":
        # Again as an icon: a round arrow going back on itself
        cx = bx - 84 - 10 - 20
        s.append(f'<circle cx="{cx}" cy="{top+48}" r="20" fill="rgba(255,255,255,.09)"/>'
                 f'<path transform="translate({cx-12} {top+36})" fill="#e6e6e6" d="{AGAIN}"/>')
    else:
        s.append(button(bx - 84 - 10 - 80, top + 28, 80, "Again"))
    if menu and more_x is not None:
        mx, my = more_x - 200, top - 108
        s.append(f'<rect x="{mx}" y="{my}" width="220" height="96" rx="14" fill="#313135" fill-opacity=".97" stroke="#fff" stroke-opacity=".1"/>'
                 f'<text x="{mx+16}" y="{my+33}" font-size="15" fill="#fff">Use the seed <tspan fill="#9a9a9a">18234</tspan></text>'
                 f'<rect x="{mx}" y="{my+48}" width="220" height="1" fill="#fff" fill-opacity=".12"/>'
                 f'<text x="{mx+16}" y="{my+78}" font-size="15" fill="#ff6b6b">Clear all results…</text>')
    return "\n".join(s)


def tab(label="Results · 4", art="art-r2", running=None):
    w = round(max(170, 82 + 8.4 * len(label)))
    x, y = 560 - w / 2, 740
    s = [f'<rect x="{x}" y="{y}" width="{w}" height="42" rx="21" fill="#202020" fill-opacity=".94"/>']
    if running is not None:
        cx = x + 23
        s.append(f'<circle cx="{cx}" cy="{y+21}" r="12" fill="none" stroke="#414141" stroke-width="3"/>'
                 f'<circle cx="{cx}" cy="{y+21}" r="12" fill="none" stroke="{ACCENT}" stroke-width="3" stroke-dasharray="{75.4*running/100:.1f} 76" stroke-linecap="round" transform="rotate(-90 {cx} {y+21})"/>')
    else:
        s.append(thumb(x + 8, y + 7, art, size=28))
    s.append(f'<text x="{x+46}" y="{y+26}" font-size="14.5" font-weight="500" fill="#ededed">{esc(label)}</text>')
    s.append(f'<path d="M{x+w-28} {y+24} l6 -6 l6 6" stroke="#bdbdbd" stroke-width="2.2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>')
    return "\n".join(s)


def note(x, y, w, lines, arrow=None):
    h = 14 + 17 * len(lines)
    s = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#ffe36e"/>']
    for i, line in enumerate(lines):
        s.append(f'<text x="{x+10}" y="{y+21+17*i}" font-size="13" fill="#222">{esc(line)}</text>')
    if arrow:
        ax, ay = arrow
        sx = x + w / 2
        sy = y + h if ay > y else y
        s.append(f'<path d="M{sx} {sy} L{ax} {ay}" stroke="#ffe36e" stroke-width="2.5" stroke-dasharray="6 5"/>'
                 f'<circle cx="{ax}" cy="{ay}" r="5" fill="#ffe36e"/>')
    return "\n".join(s)


def tap(x, y):
    return (f'<circle cx="{x}" cy="{y}" r="22" fill="#fff" fill-opacity=".25" stroke="#fff" stroke-width="2"/>'
            f'<circle cx="{x}" cy="{y}" r="7" fill="#fff"/>')


def popover():
    x, y, w = 60, 56, 400
    return (f'<rect x="{x}" y="{y}" width="{w}" height="300" rx="22" fill="#1e1e1e" fill-opacity=".9" stroke="#fff" stroke-opacity=".08"/>'
            f'<text x="{x+16}" y="{y+40}" font-size="24" font-weight="700" fill="#c7c7c7">Generate</text>'
            f'<text x="{x+16}" y="{y+76}" font-size="12" fill="#9a9a9a" letter-spacing="1">PROMPT</text>'
            f'<rect x="{x+16}" y="{y+86}" width="{w-32}" height="76" rx="12" fill="#fff" fill-opacity=".07"/>'
            f'<text x="{x+28}" y="{y+112}" font-size="15" fill="#fff">a red lighthouse on a rocky coast,</text>'
            f'<text x="{x+28}" y="{y+132}" font-size="15" fill="#fff">dusk</text>'
            f'<rect x="{x+16}" y="{y+238}" width="{w-32}" height="46" rx="14" fill="{ACCENT}"/>'
            f'<text x="{x+w/2}" y="{y+267}" font-size="16" font-weight="600" text-anchor="middle" fill="#fff">Generate</text>')


def scene(canvas_art, body, ai_lit=False, running=False, stroke=False):
    extra = ""
    if stroke:
        extra = '<path d="M250 380 C320 330 380 420 450 360 S560 330 600 390" stroke="#f4f0e6" stroke-width="9" fill="none" stroke-linecap="round"/>'
    return (f'<svg class="frame" viewBox="0 0 1120 800" role="img" font-family="Inter, system-ui, sans-serif">'
            f'<use href="#{canvas_art}"/>{extra}{topbar(running=running, ai_lit=ai_lit)}{body}</svg>')


S_TODAY = scene("art-r2", tray(shown=1, close="x")
                + note(330, 560, 300, ["The bar covers the bottom of the", "picture being looked at"], arrow=(420, 690))
                + note(820, 600, 250, ["✕ deletes every result for good"], arrow=(955, 728))
                + note(80, 420, 230, ["A tap here paints a stroke"]) + tap(190, 500))

S_OPEN = scene("art-r2", tray(shown=1)
               + tap(640, 330)
               + note(670, 300, 330, ["Tap anywhere on the canvas: the tray", "tucks away. Nothing is painted."], arrow=(660, 330))
               + note(860, 590, 220, ["⌄ tucks it away too", "(desktop: Esc)"], arrow=(944, 730)))

S_TUCKED = scene("art-r2", tab()
                 + note(330, 560, 320, ["The whole picture, uncovered: the", "preview stays on the canvas"])
                 + note(700, 660, 330, ["Tap the tab: the tray comes back,", "with every result still in it"], arrow=(640, 760)))

S_PAINT = scene(
    "art-original", tab(art="art-r2")
    + note(620, 280, 350, ["Drawing works as normal with the tray", "tucked: the first stroke hides the",
                           "preview, as today. The tab stays."]), stroke=True)

S_MENU = scene("art-r2", tray(shown=1, menu=True, deleting=3)
               + note(120, 560, 250, ["Hold a thumbnail: its own Delete,", "as today"], arrow=(378, 662))
               + note(470, 470, 260, ["··· holds what throws them all", "away, and still asks first"], arrow=(720, 600)))

S_RUN = scene("art-original", popover() + tab(label="Generating · 63%", running=63)
              + note(640, 600, 360, ["Generate with the tray tucked: the tab", "shows the progress, and the tray comes", "up by itself when the first one is done"],
                     arrow=(600, 742)), ai_lit=True, running=True)

HTML = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Results tray</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
<style>
:root {{ --page: #121212; --text: #e8e8e8; --dim: #9a9a9a; --rule: #2a2a2a; --card: #1e1e1e; --ok: #3ccf6e; }}
@media (prefers-color-scheme: light) {{
  :root:not([data-theme="dark"]) {{ --page: #f4f4f4; --text: #1d1d1d; --dim: #666; --rule: #ddd; --card: #fff; }}
}}
:root[data-theme="light"] {{ --page: #f4f4f4; --text: #1d1d1d; --dim: #666; --rule: #ddd; --card: #fff; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--page); color: var(--text); font-family: Inter, system-ui, sans-serif; }}
main {{ max-width: 1180px; margin: 0 auto; padding: 32px 16px 64px; }}
h1 {{ font-size: 28px; margin: 0 0 6px; }}
.lede {{ color: var(--dim); margin: 0 0 28px; max-width: 780px; line-height: 1.5; }}
section {{ margin: 0 0 44px; }}
section > h2 {{ font-size: 18px; margin: 0 0 4px; }}
section > p {{ color: var(--dim); margin: 0 0 12px; font-size: 14px; line-height: 1.5; max-width: 820px; }}
.frame {{ width: 100%; height: auto; display: block; border-radius: 14px; box-shadow: 0 6px 30px rgba(0,0,0,.35); background: #1c1c1c; }}
.rules {{ background: var(--card); border-radius: 14px; padding: 18px 22px; line-height: 1.6; font-size: 14.5px; }}
.rules li {{ margin: 4px 0; }}
.q {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }}
.q > div {{ background: var(--card); border-radius: 14px; padding: 16px 18px; font-size: 14px; line-height: 1.5; }}
.q h4 {{ margin: 0 0 6px; font-size: 15px; }}
.q .rec {{ color: var(--ok); font-weight: 600; }}
@media (max-width: 700px) {{ .q {{ grid-template-columns: minmax(0, 1fr); }} }}
</style>
</head>
<body>
{DEFS}
<main>
<h1>The results tray: tuck it away, never lose it</h1>
<p class="lede">Sketches only, nothing built. Direction A from our brainstorm. The test tablet's screen at 1120 × 800 dp, in ProCraft's colours and sizes. The idea: the <b>results tray</b> (the bar of results at the bottom) is never <i>closed</i>, only <b>tucked away</b> into a small tab, and it comes back with everything still in it. Updated with your answers: the seed moved into ···, and Again is an icon. The desktop works the same with a mouse.</p>

<section>
  <h2>Today, for comparison</h2>
  <p>The bar covers the bottom of the picture you're looking at, a tap on the canvas paints, and ✕ deletes every result for good.</p>
  {S_TODAY}
</section>

<section>
  <h2>1 · After a run: the tray is up</h2>
  <p>Quieter than today: thumbnails, then ↻ (Again), Apply, ··· and ⌄. The seed tag and ✕ are gone: the seed is copied from ···, and ⌄ tucks the tray away. While the tray is up, the canvas doesn't paint: a tap on it tucks the tray away, as a tap outside a popover closes it.</p>
  {S_OPEN}
</section>

<section>
  <h2>2 · Tucked away: the whole picture, and a tab</h2>
  <p>The preview stays on the canvas, now uncovered. A small tab at the bottom shows the shown result and how many there are. Tapping it brings the tray back. The tab is there whenever the artwork has results, even after leaving it and coming back, or restarting the app.</p>
  {S_TUCKED}
</section>

<section>
  <h2>3 · Drawing with the tray tucked</h2>
  <p>With the tray tucked, the canvas is yours again. The first stroke hides the preview, as it does today. The tab stays, so the results are one tap away.</p>
  {S_PAINT}
</section>

<section>
  <h2>4 · Throwing results away, on purpose</h2>
  <p>Holding a thumbnail still gives its own Delete. ··· holds "Use the seed" (of the result shown: it goes in the popover, locked) and "Clear all results…", which still asks first. Clearing is no longer the button you press to get the tray out of the way.</p>
  {S_MENU}
</section>

<section>
  <h2>5 · Generating with the tray tucked</h2>
  <p>The tab becomes the progress: a ring and "Generating · 63%". When the first picture is ready, the tray comes up by itself to show it, as it does today.</p>
  {S_RUN}
</section>

<section>
  <h2>The rules, in short</h2>
  <div class="rules"><ul>
    <li><b>Up</b> after a run, or when the tab is tapped. While it's up, a tap on the canvas only tucks it (nothing painted).</li>
    <li><b>Tucked</b> with ⌄, a tap on the canvas, or Esc on desktop. The preview stays; the first stroke hides it.</li>
    <li><b>The tab</b> shows while the artwork has results: the shown one's thumbnail and the count, or the progress while generating.</li>
    <li><b>Nothing is lost</b> by tucking. Results go only through a thumbnail's Delete, or "Clear all results…" in ···, which asks first.</li>
    <li><b>Hides with other tools' bars</b> (Adjustments, Transform, the Selection toolbar) as today, and comes back after, tray or tab as it was.</li>
  </ul></div>
</section>

<section>
  <h2>Decided so far (2026-11-06)</h2>
  <div class="q">
    <div><h4>Its name</h4>The <b>results tray</b>. Its tab reads "Results · 4" (short, so it stays small).</div>
    <div><h4>Where the tab sits</h4><b>Bottom centre</b>, where the tray was.</div>
    <div><h4>The seed</h4>Out of the tray, into ···: "Use the seed 18234".</div>
    <div><h4>Again</h4>An icon, a round arrow (↻), not a text button. Apply stays the one blue text button.</div>
  </div>
  <p style="margin-top:12px">Still open: whether the Generate popover also lists the results. Suggested: not now, the tab already brings them back.</p>
</section>
</main>
</body>
</html>
"""

with open(OUT, "w") as f:
    f.write(HTML)
print("wrote", OUT)
