#!/usr/bin/env python3
"""Writes docs/generation/mockups/style-editor.html: static SVG scenes, no scripts."""
import sys

OUT = sys.argv[1]
BASE = sys.argv[2]  # the tray mockup's generator, for its drawing helpers
src = open(BASE).read()
exec(src[: src.index("S_TODAY = scene(")].replace("OUT = sys.argv[1]", "OUT = None"))

GLASS = "#1e1e1e"
ROW = "rgba(255,255,255,.06)"
DIM = "#9a9a9a"


def t(x, y, s, size=14, fill="#fff", weight=400, anchor="start"):
    return f'<text x="{x}" y="{y}" font-size="{size}" font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">{esc(s)}</text>'


def label(x, y, s):
    return t(x, y, s.upper(), 12, DIM, 500).replace("<text ", '<text letter-spacing="1" ')


def field(x, y, w, h, lines, mono=False):
    s = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="#fff" fill-opacity=".07"/>']
    for i, line in enumerate(lines):
        s.append(t(x + 12, y + 22 + 19 * i, line, 14))
    return "".join(s)


def row(x, y, w, left, right="", chevron=True, h=40):
    s = f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="{ROW}"/>' + t(x + 12, y + h / 2 + 5, left, 14)
    if right:
        s += t(x + w - (28 if chevron else 12), y + h / 2 + 5, right, 14, DIM, anchor="end")
    if chevron:
        s += t(x + w - 14, y + h / 2 + 5, "›", 18, DIM, anchor="end")
    return s


def seg(x, y, w, names, on):
    n = len(names)
    cw = (w - 6) / n
    s = [f'<rect x="{x}" y="{y}" width="{w}" height="36" rx="11" fill="{ROW}"/>']
    for i, name in enumerate(names):
        cx = x + 3 + i * cw
        if i == on:
            s.append(f'<rect x="{cx}" y="{y+3}" width="{cw}" height="30" rx="9" fill="{ACCENT}"/>')
        s.append(t(cx + cw / 2, y + 23, name, 13.5, "#fff" if i == on else DIM, anchor="middle"))
    return "".join(s)


def slider(x, y, w, frac, name, value):
    return (t(x, y, name, 14, "#c1c1c1") + t(x + w, y, value, 14, "#ededed", anchor="end")
            + f'<rect x="{x}" y="{y+12}" width="{w}" height="4" rx="2" fill="#414141"/>'
            + f'<rect x="{x}" y="{y+12}" width="{w*frac}" height="4" rx="2" fill="#ededed"/>'
            + f'<rect x="{x + w*frac - 15}" y="{y+5}" width="30" height="18" rx="9" fill="#e9e9e9"/>')


def switch(x, y, on):
    return (f'<rect x="{x}" y="{y}" width="50" height="30" rx="15" fill="{ACCENT if on else "#747377"}"/>'
            f'<circle cx="{x + (35 if on else 15)}" cy="{y+15}" r="12" fill="#fff"/>')


def lora(x, y, w, name, frac, value):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="62" rx="12" fill="{ROW}"/>' + t(x + 12, y + 23, name, 14)
            + t(x + w - 12, y + 23, "×", 18, DIM, anchor="end")
            + f'<rect x="{x+12}" y="{y+40}" width="{w-80}" height="4" rx="2" fill="#414141"/>'
            + f'<rect x="{x+12}" y="{y+40}" width="{(w-80)*frac}" height="4" rx="2" fill="#ededed"/>'
            + f'<rect x="{x+12+(w-80)*frac-15}" y="{y+33}" width="30" height="18" rx="9" fill="#e9e9e9"/>'
            + t(x + w - 12, y + 47, value, 13, DIM, anchor="end"))


def pop(x, y, w, h, arrow_x=None):
    s = f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="22" fill="{GLASS}" fill-opacity=".94" stroke="#fff" stroke-opacity=".08"/>'
    if arrow_x:
        s += f'<path d="M{arrow_x-10} {y+1} L{arrow_x} {y-10} L{arrow_x+10} {y+1}z" fill="{GLASS}" fill-opacity=".94"/>'
    return s


def cards(x, y, on=None, held=None):
    s = []
    for i, (name, sub) in enumerate([("Anima", "30 steps · CFG 4"), ("Anima Turbo", "8 steps · CFG 1"), ("Moonlit", "LoRA · 24 steps")]):
        cx = x + i * 124
        s.append(f'<rect x="{cx}" y="{y}" width="116" height="76" rx="12" fill="#5b5b5b" fill-opacity=".18" stroke="{ACCENT if i == on else "none"}" stroke-width="2"/>')
        s.append(t(cx + 10, y + 26, name, 14.5, "#fff", 500))
        s.append(t(cx + 10, y + 64, sub, 11.5, DIM))
        if i == held:
            s.append(f'<rect x="{cx-3}" y="{y-3}" width="122" height="82" rx="14" fill="none" stroke="#ffe36e" stroke-width="2.5" stroke-dasharray="5 4"/>')
    return "".join(s)


def menu(x, y, items):
    h = 14 + 44 * len(items)
    s = [f'<rect x="{x}" y="{y}" width="200" height="{h}" rx="14" fill="#313135" fill-opacity=".98" stroke="#fff" stroke-opacity=".1"/>']
    for i, (name, colour) in enumerate(items):
        if i:
            s.append(f'<rect x="{x}" y="{y+7+44*i}" width="200" height="1" fill="#fff" fill-opacity=".1"/>')
        s.append(t(x + 16, y + 36 + 44 * i, name, 15, colour))
    return "".join(s)


def frame(body, canvas="art-original"):
    return (f'<svg class="frame" viewBox="0 0 1120 800" role="img" font-family="Inter, system-ui, sans-serif">'
            f'<use href="#{canvas}"/>{topbar(ai_lit=True)}{body}</svg>')


PX, PW = 60, 400   # the popover's left and width, as the Generate popover
IX, IW = PX + 16, PW - 32

# 0 · the long-press menu on a style card
S_MENU = frame(
    pop(PX, 56, PW, 300, arrow_x=320)
    + t(PX + 16, 96, "Generate", 24, "#c7c7c7", 700)
    + label(PX + 18, 132, "Style") + cards(IX, 142, on=2, held=2)
    + menu(IX + 250, 236, [("Edit", "#fff"), ("Duplicate", "#fff"), ("Delete", "#ff6b6b")])
    + menu(PX + PW + 40, 236, [("Duplicate", "#fff")])
    + note(PX + PW + 40, 160, 330, ["Holding a built-in style (Anima, Anima Turbo)", "offers only Duplicate: the copy is yours"])
    + note(IX + 10, 470, 330, ["Holding one of yours: Edit, Duplicate, Delete"], arrow=(IX + 330, 300)))

# A1 · the popover page, top
def page_top():
    y = 56
    s = [pop(PX, y, PW, 730, arrow_x=320)]
    s.append(t(PX + 16, y + 40, "‹", 26, ACCENT) + t(PX + 40, y + 40, "Moonlit", 22, "#c7c7c7", 700)
             + t(PX + PW - 16, y + 38, "Done", 16, ACCENT, 500, anchor="end"))
    yy = y + 70
    s.append(label(IX + 2, yy, "Name")); yy += 10
    s.append(field(IX, yy, IW, 40, ["Moonlit"])); yy += 56
    s.append(label(IX + 2, yy, "Model")); yy += 10
    s.append(row(IX, yy, IW, "anima-base-v1.0", "")); yy += 56
    s.append(label(IX + 2, yy, "LoRAs")); yy += 10
    s.append(lora(IX, yy, IW, "moonlight_style_v2", 0.7, "0.70")); yy += 68
    s.append(lora(IX, yy, IW, "watercolor_lite", 0.35, "0.35")); yy += 68
    s.append(f'<rect x="{IX}" y="{yy}" width="{IW}" height="40" rx="12" fill="{ROW}"/>' + t(IX + IW / 2, yy + 26, "+  Add LoRA", 14, "#dcdcdc", anchor="middle")); yy += 56
    s.append(label(IX + 2, yy, "Prompt")); yy += 10
    s.append(field(IX, yy, IW, 60, ["masterpiece, best quality, {prompt},", "night, moonlight, soft glow"])); yy += 76
    s.append(label(IX + 2, yy, "Negative prompt")); yy += 10
    s.append(field(IX, yy, IW, 60, ["worst quality, low quality, blurry,", "jpeg artifacts, watermark"])); yy += 76
    s.append(label(IX + 2, yy, "Detail")); yy += 10
    s.append(seg(IX, yy, IW, ["Fast", "Balanced", "Fine"], 1)); yy += 52
    s.append(t(IX + 2, yy + 20, "Advanced", 15, "#fff") + switch(IX + IW - 50, yy, False))
    return "".join(s)


S_A1 = frame(page_top()
             + note(PX + PW + 30, 200, 360, ["A · a page inside the Generate popover, as the", "server view is. It scrolls; ‹ goes back to", "the form. The canvas stays in view on the right."])
             + note(PX + PW + 30, 420, 330, ["“Detail” sets steps and the sampler together;", "Advanced shows the real numbers"], arrow=(IX + IW - 20, 700)))


def page_advanced():
    y = 56
    s = [pop(PX, y, PW, 730, arrow_x=320)]
    s.append(t(PX + 16, y + 40, "‹", 26, ACCENT) + t(PX + 40, y + 40, "Moonlit", 22, "#c7c7c7", 700)
             + t(PX + PW - 16, y + 38, "Done", 16, ACCENT, 500, anchor="end"))
    yy = y + 64
    s.append(t(IX + 2, yy + 20, "Advanced", 15, "#fff") + switch(IX + IW - 50, yy, True)); yy += 48
    s.append(row(IX, yy, IW, "Sampler", "Euler ancestral")); yy += 48
    s.append(row(IX, yy, IW, "Scheduler", "Normal")); yy += 60
    s.append(slider(IX + 4, yy, IW - 8, 0.4, "Steps", "24")); yy += 48
    s.append(slider(IX + 4, yy, IW - 8, 0.3, "CFG", "4.0")); yy += 48
    s.append(slider(IX + 4, yy, IW - 8, 0.0, "Rescale CFG", "0")); yy += 40
    s.append(row(IX, yy, IW, "VAE", "The model's own")); yy += 48
    s.append(row(IX, yy, IW, "Text encoder", "qwen_3_06b_base")); yy += 48
    s.append(row(IX, yy, IW, "Best size", "About 1 MP")); yy += 48
    s.append(row(IX, yy, IW, "Seed", "Random")); yy += 64
    s.append(f'<rect x="{IX}" y="{yy}" width="{IW}" height="40" rx="12" fill="{ROW}"/>' + t(IX + IW / 2, yy + 26, "Duplicate style", 14, "#fff", anchor="middle")); yy += 48
    s.append(f'<rect x="{IX}" y="{yy}" width="{IW}" height="40" rx="12" fill="{ROW}"/>' + t(IX + IW / 2, yy + 26, "Delete style", 14, "#ff6b6b", anchor="middle"))
    return "".join(s)


S_A2 = frame(page_advanced()
             + note(PX + PW + 30, 150, 370, ["Advanced on: everything Krita's style editor", "has that Anima uses. (Clip skip, v-prediction and", "live-painting settings are for other model kinds.)"]))


# B · the full-screen studio
def studio():
    s = ['<rect x="0" y="0" width="1120" height="800" fill="#161616"/>',
         '<rect x="0" y="0" width="1120" height="56" fill="#1c1c1c"/>',
         t(32, 36, "Cancel", 16, ACCENT), t(560, 36, "Style Studio", 18, "#c7c7c7", 600, anchor="middle"),
         t(1088, 36, "Done", 16, ACCENT, 600, anchor="end")]
    # left: the sections
    s.append('<rect x="16" y="72" width="220" height="712" rx="18" fill="#1e1e1e"/>')
    for i, name in enumerate(["Model", "LoRAs", "Prompts", "Detail", "Advanced", "About"]):
        yy = 92 + i * 52
        if i == 1:
            s.append(f'<rect x="26" y="{yy}" width="200" height="42" rx="10" fill="{ACCENT}"/>')
        s.append(t(44, yy + 27, name, 15.5, "#fff" if i == 1 else "#c7c7c7"))
    # middle: the section shown
    s.append('<rect x="252" y="72" width="460" height="712" rx="18" fill="#1e1e1e"/>')
    s.append(t(276, 112, "LoRAs", 22, "#c7c7c7", 700))
    s.append(lora(276, 132, 412, "moonlight_style_v2", 0.7, "0.70"))
    s.append(lora(276, 202, 412, "watercolor_lite", 0.35, "0.35"))
    s.append(f'<rect x="276" y="272" width="412" height="40" rx="12" fill="{ROW}"/>' + t(482, 298, "+  Add LoRA", 14, "#dcdcdc", anchor="middle"))
    s.append(label(278, 346, "On the server"))
    for i, name in enumerate(["detailed_eyes_v3", "flat_color_anime", "ink_wash", "pastel_dream", "retro_90s"]):
        s.append(row(276, 358 + i * 48, 412, name, "+", chevron=False))
    # right: a preview
    s.append('<rect x="728" y="72" width="376" height="712" rx="18" fill="#1e1e1e"/>')
    s.append(t(752, 112, "Preview", 22, "#c7c7c7", 700))
    s.append('<svg x="752" y="132" width="328" height="328" viewBox="200 0 800 800" preserveAspectRatio="xMidYMid slice"><use href="#art-r4"/></svg>')
    s.append(field(752, 476, 328, 60, ["a lighthouse on a cliff, scenery,", "no humans"]))
    s.append(f'<rect x="752" y="552" width="328" height="44" rx="14" fill="{ACCENT}"/>' + t(916, 580, "Try it", 15, "#fff", 600, anchor="middle"))
    s.append(t(752, 624, "Makes one small picture with this style,", 12.5, DIM))
    s.append(t(752, 642, "so you see what a change does.", 12.5, DIM))
    return "".join(s)


S_B = (f'<svg class="frame" viewBox="0 0 1120 800" role="img" font-family="Inter, system-ui, sans-serif">{studio()}'
       + note(300, 640, 370, ["B · a full screen of its own, as Brush Studio is:", "sections on the left, room for the LoRAs list,", "and a preview that tries the style out"]) + '</svg>')

HTML = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Style editor</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
<style>
:root {{ --page: #121212; --text: #e8e8e8; --dim: #9a9a9a; --card: #1e1e1e; --ok: #3ccf6e; }}
@media (prefers-color-scheme: light) {{
  :root:not([data-theme="dark"]) {{ --page: #f4f4f4; --text: #1d1d1d; --dim: #666; --card: #fff; }}
}}
:root[data-theme="light"] {{ --page: #f4f4f4; --text: #1d1d1d; --dim: #666; --card: #fff; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--page); color: var(--text); font-family: Inter, system-ui, sans-serif; }}
main {{ max-width: 1180px; margin: 0 auto; padding: 32px 16px 64px; }}
h1 {{ font-size: 28px; margin: 0 0 6px; }}
.lede {{ color: var(--dim); margin: 0 0 28px; max-width: 780px; line-height: 1.5; }}
section {{ margin: 0 0 44px; }}
section > h2 {{ font-size: 18px; margin: 0 0 4px; }}
section > p {{ color: var(--dim); margin: 0 0 12px; font-size: 14px; line-height: 1.5; max-width: 820px; }}
.frame {{ width: 100%; height: auto; display: block; border-radius: 14px; box-shadow: 0 6px 30px rgba(0,0,0,.35); background: #1c1c1c; }}
.cmp {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }}
.cmp > div {{ background: var(--card); border-radius: 14px; padding: 16px 18px; font-size: 14px; line-height: 1.55; }}
.cmp h4 {{ margin: 0 0 6px; font-size: 15px; }}
.cmp ul {{ margin: 6px 0 0; padding-left: 18px; }}
.list {{ background: var(--card); border-radius: 14px; padding: 16px 22px; font-size: 14px; line-height: 1.6; }}
@media (max-width: 700px) {{ .cmp {{ grid-template-columns: minmax(0, 1fr); }} }}
</style>
</head>
<body>
{DEFS}
<main>
<h1>Editing styles: a page in the popover, or a studio?</h1>
<p class="lede"><b>Decided 2026-11-06: B, the Style Studio.</b> Someone working on a style doesn't need the canvas, and krita-ai-diffusion edits styles in a window of its own. A stays here for the record.</p>
<p class="lede">Sketches only, nothing built. The test tablet's screen at 1120 × 800 dp. The same style, "Moonlit" (Anima with two LoRAs), edited both ways. Agreed already: built-in styles are read-only (Duplicate makes yours), plain words first with the numbers under Advanced, styles are app-wide.</p>

<section>
  <h2>Getting there: hold a style card</h2>
  <p>In the Generate popover, holding a style card opens a small menu, as holding a brush does. Your own styles: Edit, Duplicate, Delete. Built-in ones: Duplicate only.</p>
  {S_MENU}
</section>

<section>
  <h2>A · A page inside the popover: the top</h2>
  <p>Edit swaps the form for the style's page, as the server button swaps it for the server view. ‹ goes back. Everything is one scrolling column.</p>
  {S_A1}
</section>

<section>
  <h2>A · The same page, Advanced on</h2>
  <p>The numbers behind "Detail", and the rarer settings. At the bottom, Duplicate and Delete.</p>
  {S_A2}
</section>

<section>
  <h2>B · A full-screen Style Studio <span style="color:var(--ok);font-size:13px">(chosen)</span></h2>
  <p>Edit opens a screen of its own, as Brush Studio does for brushes. Sections on the left (Model, LoRAs, Prompts, Detail, Advanced, About), the section in the middle, and on the right a preview: "Try it" makes one small picture with the style as it is now.</p>
  {S_B}
</section>

<section>
  <h2>Side by side</h2>
  <div class="cmp">
    <div><h4>A · Page in the popover</h4><ul>
      <li>Quick: two taps from the cards, and the canvas stays in view.</li>
      <li>Same place and look as the server view: nothing new to learn.</li>
      <li>One long column: with several LoRAs and Advanced on, a lot of scrolling.</li>
      <li>No room for a preview; you try a change by generating on the canvas.</li>
      <li>Less to build.</li>
    </ul></div>
    <div><h4>B · Style Studio</h4><ul>
      <li>Room: the LoRA list, the server's LoRAs to pick from, and a preview side by side.</li>
      <li>Feels like Brush Studio, ProCraft's own way of editing a brush.</li>
      <li>Leaves the canvas: you don't see your artwork while editing.</li>
      <li>"Try it" needs its own small run (a prompt just for previews).</li>
      <li>More to build, on both apps.</li>
    </ul></div>
  </div>
</section>

<section>
  <h2>What a style holds (both options)</h2>
  <div class="list">Name · model (checkpoint) · LoRAs, each with a strength · prompt template with <code>{{prompt}}</code> · negative prompt · Detail (Fast · Balanced · Fine), and under Advanced: sampler, scheduler, steps, CFG, rescale CFG, VAE, text encoder, best size, seed. As Krita's style editor, less the settings Anima doesn't use (clip skip, v-prediction, the live-painting sampler).</div>
</section>
</main>
</body>
</html>
"""

with open(sys.argv[1], "w") as f:
    f.write(HTML)
print("wrote", sys.argv[1])
