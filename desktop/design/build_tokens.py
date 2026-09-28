"""Writes the DTCG 2025.10 token files and a CSS mirror for the HTML mockups from one table.

Stand-in for the Style Dictionary build until the desktop repo exists; the JSON it writes is the
source of truth, so hand edits go into the tables below, never into the generated files.
"""
import json, pathlib

OUT = pathlib.Path(__file__).parent / "tokens"

def col(v):
    """'#rrggbb' or ('#rrggbb', alpha) -> DTCG color value."""
    hx, a = (v, 1.0) if isinstance(v, str) else v
    c = [round(int(hx[i:i + 2], 16) / 255, 4) for i in (1, 3, 5)]
    out = {"colorSpace": "srgb", "components": c, "hex": hx.lower()}
    if a != 1.0:
        out["alpha"] = a
    return out

def px(n):
    return {"value": n, "unit": "px"}

W, K, NAVY = "#ffffff", "#000000", "#191e50"

# name: (light, dark, description)
COLORS = {
    "surface.window":        (W, "#1f1f22", "Opaque window and raised card fill (Solid material)"),
    "surface.sidebar":       (("#f2f2f6", .86), ("#2c2c30", .86), "Sidebar pane over the window"),
    "surface.code":          ("#f6f6f8", "#28282c", "Code blocks, tool command wells"),
    "surface.capsule":       ((W, .72), ("#323236", .72), "Toolbar capsules and segmented controls"),
    "surface.capsuleBorder": ((K, .08), (W, .10), "Hairline around capsules"),
    "surface.popover":       (("#fafbff", .975), ("#2a2b31", .96), "Popovers; text-bearing so near-opaque"),
    "surface.terminal":      (("#181b28", .88), ("#101218", .92), "Inset terminal tail"),
    "fill.primary":          ((K, .045), (W, .06), "Quiet fills: user bubble (solid), filter field"),
    "fill.secondary":        ((K, .07), (W, .10), "Icon tiles, spinner track"),
    "text.primary":          ("#1d1d1f", "#f2f2f5", "Body text"),
    "text.secondary":        ("#6e6e73", "#a1a1a8", "Metadata, subtitles"),
    "text.tertiary":         ("#a1a1a6", "#6c6c72", "Timestamps, placeholders, key caps"),
    "text.terminal":         ("#c8cde0", "#c8cde0", "Terminal tail text"),
    "text.terminalOk":       ("#8ff0a8", "#8ff0a8", "Passing lines in a terminal tail"),
    "separator":             ((K, .09), (W, .09), "Hairlines, 0.5 pt"),
    "accent":                ("#0a7cff", "#3b9bff", "Primary actions, links, selection"),
    "accent.soft":           (("#0a7cff", .12), ("#3b9bff", .18), "Selected row, focus halo"),
    "status.success":        ("#1f9d45", "#3bd06a", "Done, passing, running dot, received tokens"),
    "status.successSoft":    (("#34c759", .14), ("#30d158", .16), "Halo around the running dot"),
    "status.warning":        ("#f08a00", "#ffa53a", "Needs you, approvals, risk"),
    "status.warningSoft":    (("#ff9500", .12), ("#ff9f0a", .16), "Risk chip, pinned banner"),
    "status.danger":         ("#e5352b", "#ff6259", "Errors, deny, bypass mode"),
    "status.dangerSoft":     (("#ff3b30", .11), ("#ff453a", .16), "Error banners"),
    "status.plan":           ("#2f6fde", "#5b93ff", "Plan mode"),
    "diff.add":              (("#34c759", .16), ("#30d158", .14), "Added line"),
    "diff.addGutter":        (("#34c759", .32), ("#30d158", .28), "Added line number cell"),
    "diff.del":              (("#ff3b30", .13), ("#ff453a", .14), "Removed line"),
    "diff.delGutter":        (("#ff3b30", .30), ("#ff453a", .28), "Removed line number cell"),
    "syntax.keyword":        ("#ad3da4", "#ff7ab2", ""),
    "syntax.string":         ("#c41a16", "#ff8170", ""),
    "syntax.number":         ("#1c00cf", "#d9c97c", ""),
    "syntax.function":       ("#326d74", "#67b7a4", ""),
    "syntax.comment":        ("#707f8c", "#7f8c98", ""),
    "syntax.type":           ("#0b4f79", "#5dd8ff", ""),
    "meter.sent":            ("#0a6cf0", "#3b9bff", "Tokens sent (↑)"),
    "meter.received":        ("#1f9d45", "#3bd06a", "Tokens received (↓) and speed sparkline"),
    "context.system":        ("#8a93b8", "#8a93b8", "Context bar: system prompt"),
    "context.tools":         ("#b58cff", "#b58cff", "Context bar: tool schemas"),
    "context.instructions":  ("#5fc8ff", "#5fc8ff", "Context bar: AGENTS.md / CLAUDE.md"),
    "context.history":       ("#0a6cf0", "#3b9bff", "Context bar: conversation history"),
    "shadow.tint":           (NAVY, "#000000", "Shadow colour; navy keeps glass depth cool, not grey"),
}

# Tool icon tiles: (top, bottom, glyph) — a two-stop vertical gradient, same in both appearances.
TILES = {
    "neutral": ("#ffffff", "#e4e8f3", "#3d4466"),
    "edit":    ("#5aa8ff", "#1668e3", "#ffffff"),
    "shell":   ("#4a4f63", "#1d2030", "#9ef0b5"),
    "search":  ("#6fe0d0", "#1aa39a", "#ffffff"),
    "write":   ("#c7a6ff", "#7c4ddb", "#ffffff"),
}

SPACE = {"xxs": 2, "xs": 4, "s": 6, "m": 8, "ml": 10, "l": 12, "xl": 16, "xxl": 20, "xxxl": 24, "huge": 32}
RADIUS = {"xs": 4, "s": 6, "m": 8, "l": 10, "xl": 12, "xxl": 14, "panel": 16, "pane": 18, "popover": 20, "window": 22, "capsule": 999}
SIZE = {
    "toolbarHeight": 56, "sidebarWidth": 252, "inspectorWidth": 324, "readingWidth": 760, "paneGap": 8,
    "capsuleHeight": 32, "buttonHeight": 28, "buttonHeightSmall": 24, "iconTile": 22, "statusDot": 9,
    "hairline": 0.5, "windowMinWidth": 1100, "windowMinHeight": 700, "popoverWidth": 340, "tokenPopoverWidth": 360,
}
# name: (size pt, weight, line height multiplier, tracking em, family, note)
TYPE = {
    "title.window":   (13.5, 600, 1.3,  0,     "text", "Session title in the toolbar"),
    "body":           (13,   400, 1.45, 0,     "text", "Default UI text"),
    "transcript":     (13.5, 400, 1.55, 0,     "text", "Assistant and user messages"),
    "transcript.h3":  (15,   600, 1.35, 0,     "text", "Markdown headings in the transcript"),
    "control":        (12.5, 500, 1.2,  0,     "text", "Buttons, capsules, segmented items"),
    "caption":        (12,   400, 1.4,  0,     "text", "Thinking line, notices"),
    "footnote":       (11.5, 400, 1.4,  0,     "text", "Tool status, meter, key-value rows"),
    "label":          (11,   600, 1.3,  0.03,  "text", "Uppercase section headers"),
    "micro":          (10.5, 500, 1.3,  0,     "text", "Key caps, legends, badges"),
    "metric":         (26,   650, 1.0, -0.02,  "text", "Big numbers (tok/s); tabular digits"),
    "mono.code":      (12,   400, 1.55, 0,     "mono", "Code blocks, diffs"),
    "mono.inline":    (12,   400, 1.3,  0,     "mono", "Inline code in prose"),
    "mono.terminal":  (11.5, 400, 1.5,  0,     "mono", "Terminal tail"),
}
FAMILY = {"text": ["SF Pro Text", "-apple-system", "system-ui"], "mono": ["SF Mono", "ui-monospace", "Menlo"]}

def sh(y, blur, a, x=0, spread=0, inset=False, color=NAVY):
    v = {"color": col((color, a)), "offsetX": px(x), "offsetY": px(y), "blur": px(blur), "spread": px(spread)}
    if inset:
        v["inset"] = True
    return v

HL = [sh(1, 0, .95, inset=True, color=W), sh(0, 0, .55, spread=.5, inset=True, color=W)]
ELEVATION = {
    "e0": ([], "Flat: transcript prose, collapsed tool rows"),
    "e1": ([sh(1, 2, .10), sh(4, 10, .08)] + HL, "Chips, capsules, icon tiles, selected row"),
    "e2": ([sh(2, 4, .10), sh(10, 26, .14)] + HL, "Cards: user bubble, expanded tool, sidebar, inspector"),
    "e3": ([sh(4, 10, .14), sh(24, 60, .26)] + HL, "Composer: the one thing that floats highest in a pane"),
    "e4": ([sh(8, 20, .17), sh(30, 70, .33), sh(1, 0, 1, inset=True, color=W)], "Popovers and menus"),
    "e5": ([sh(16, 40, .30, color="#080a28"), sh(50, 120, .55, color="#080a28")], "The window over the wallpaper"),
}
MATERIAL = {
    "frosted": {"windowOpacity": .42, "blur": 34, "saturation": 1.9, "specular": .35, "note": "SwiftUI Glass.regular"},
    "glossy":  {"windowOpacity": .30, "blur": 6,  "saturation": 1.9, "specular": .55, "note": "SwiftUI Glass.clear + specular streak"},
    "solid":   {"windowOpacity": 1.0, "blur": 0,  "saturation": 1.0, "specular": 0,   "note": "Opaque; forced by Reduce Transparency"},
    "readableFloor": {"windowOpacity": .80, "note": "Minimum opacity of any text-bearing panel at every setting"},
}
MOTION = {
    "duration.fast": (120, "Hover, press"), "duration.base": (200, "Disclosure expand/collapse"),
    "duration.slow": (320, "Popover, sheet, pane slide"),
    "easing.standard": ([0.2, 0, 0, 1], "Most transitions"), "easing.decelerate": ([0, 0, 0.2, 1], "Things entering"),
}

def dim_group(d, desc=""):
    return {k: {"$type": "dimension", "$value": px(v)} for k, v in d.items()}

def nest(flat):
    root = {}
    for k, v in flat.items():
        cur = root
        parts = k.split(".")
        for p in parts[:-1]:
            cur = cur.setdefault(p, {})
        cur[parts[-1]] = v
    return root

def write(name, data):
    (OUT / name).write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")

def main():
    OUT.mkdir(exist_ok=True)
    for i, mode in enumerate(("light", "dark")):
        flat = {k: {"$type": "color", "$value": col(v[i]), **({"$description": v[2]} if v[2] else {})} for k, v in COLORS.items()}
        for t, (a, b, g) in TILES.items():
            flat[f"tile.{t}.top"] = {"$type": "color", "$value": col(a)}
            flat[f"tile.{t}.bottom"] = {"$type": "color", "$value": col(b)}
            flat[f"tile.{t}.glyph"] = {"$type": "color", "$value": col(g)}
        write(f"color.{mode}.json", {"color": nest(flat)})
    typo = {}
    for k, (size, weight, lh, tr, fam, note) in TYPE.items():
        typo[k] = {"$type": "typography", "$description": note, "$value": {
            "fontFamily": FAMILY[fam], "fontSize": px(size), "fontWeight": weight,
            "lineHeight": lh, "letterSpacing": {"value": tr, "unit": "rem"}}}
    base = {
        "space": dim_group(SPACE), "radius": dim_group(RADIUS), "size": dim_group(SIZE),
        "font": nest(typo),
        "elevation": {k: {"$type": "shadow", "$description": d, "$value": v} for k, (v, d) in ELEVATION.items()},
        "motion": {"duration": {k.split(".")[1]: {"$type": "duration", "$description": d, "$value": {"value": v, "unit": "ms"}}
                                for k, (v, d) in MOTION.items() if k.startswith("duration")},
                   "easing": {k.split(".")[1]: {"$type": "cubicBezier", "$description": d, "$value": v}
                              for k, (v, d) in MOTION.items() if k.startswith("easing")}},
        "material": {k: {kk: ({"$type": "number", "$value": vv} if not isinstance(vv, str) else None)
                         for kk, vv in m.items() if kk != "note"} | {"$description": m["note"]} for k, m in MATERIAL.items()},
    }
    write("base.json", base)
    # CSS mirror for the mockups (light only; the mockups toggle .dark by hand).
    css = [":root{"]
    for k, v in COLORS.items():
        hx, a = (v[0], 1.0) if isinstance(v[0], str) else v[0]
        r, g, b = (int(hx[i:i + 2], 16) for i in (1, 3, 5))
        css.append(f"  --c-{k.replace('.', '-')}:rgba({r},{g},{b},{a});")
    css += [f"  --space-{k}:{v}px;" for k, v in SPACE.items()]
    css += [f"  --radius-{k}:{v}px;" for k, v in RADIUS.items()]
    css.append("}")
    (OUT / "tokens.css").write_text("\n".join(css) + "\n")
    print("wrote", sorted(p.name for p in OUT.iterdir()))

if __name__ == "__main__":
    main()
