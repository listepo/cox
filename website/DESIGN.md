# cox — Design System

## Overview

**cox** is a modular terminal coding agent — the coxswain that steers. Metaphor: **terminal TUI / status pane** — pane chrome, box-drawing frames, and a chartreuse cursor that signals control. Evolves the product identity toward calm nerd mono + dual theme, while staying distinct from ketch mint (cox is deeper forest + chartreuse, not bright teal-mint).

Identity: deep forest ink + chartreuse mint accents. Mark is a rounded terminal tile (`#0F1A14`) with left-pane bar + `›` cursor / block caret — readable at 16px favicon and 64px tile.

**Shared visual lock (Listepo landing v1):** **nerd + ai + glass + flat** — same family as ketch brand v1, but cox prioritizes **terminal chrome** (TUI frames, status lines, pane borders) over generic glass cards. IBM Plex Mono for UI/labels/chips; hairline borders; mono CLI panes; subtle agent/compute cues (soft accent glow, gradient hairline, status chips) using **chartreuse/forest only** (no purple AI gradients); light glass + `backdrop-filter` with opaque `@media (prefers-reduced-transparency: reduce)` fallbacks; flat CTAs, 4/8pt spacing, surface ladder, radii 8–12.

## Colors

### Light (paper terminal)

| Token | Hex | Use |
|-------|-----|-----|
| bg | `#F4F7F2` | Page background |
| bg-elevated | `#FFFFFF` | Cards, panels |
| surface-1 | `#E8EEE4` | Nested surface |
| surface-2 | `#D8E2D2` | Hover / selected |
| surface-3 | `#C5D4BC` | Pressed |
| border | `#A8BAA0` | Default border |
| border-hairline | `#D0DBC8` | Divider |
| fg | `#0F1A14` | Primary text / forest ink |
| fg-muted | `#3D4F42` | Secondary |
| fg-subtle | `#6A7D6E` | Tertiary |
| accent | `#3D8B3A` | Primary CTA |
| accent-hover | `#2F6F2C` | Hover |
| accent-muted | `#6BBF4A` | Soft accent |
| accent-soft | `#E2F0D8` | Accent wash |
| chartreuse | `#A8E06C` | Highlight / mark stroke |
| code-bg | `#0F1A14` | Code / terminal |
| code-fg | `#C8F08A` | Terminal green |

### Dark (classic green-on-ink)

| Token | Hex | Use |
|-------|-----|-----|
| bg | `#0A120E` | Page background |
| bg-elevated | `#0F1A14` | Cards, panels |
| surface-1 | `#162018` | Nested |
| surface-2 | `#1E2A22` | Hover |
| surface-3 | `#28362C` | Pressed |
| border | `#344638` | Default border |
| border-hairline | `#1C2820` | Divider |
| fg | `#E4EDE6` | Primary text |
| fg-muted | `#8FA894` | Secondary |
| fg-subtle | `#5A6E5E` | Tertiary |
| accent | `#A8E06C` | Primary CTA / chartreuse |
| accent-hover | `#C8F08A` | Hover |
| accent-muted | `#6BBF4A` | Soft accent |
| accent-soft | `#1A2A18` | Accent wash |
| chartreuse | `#C8F08A` | Highlight |
| code-bg | `#060A08` | Terminal |
| code-fg | `#C8F08A` | Prompt text |

## Typography

- **Mono (primary UI, terminal, labels, landing):** IBM Plex Mono (fallback JetBrains Mono)
- **Sans (docs body only):** IBM Plex Sans — landing leans mono; serif is gone from product chrome
- Scale: 12 / 14 / 16 / 20 / 28 / 40
- Weights: 400 body, 500 labels, 600 headings

## Layout

- Terminal-first: full-width TUI frames with titlebar + status line
- Max marketing width: ~1100px for TUI hero; content columns ~880px
- 8px grid; hairline / box-drawing borders; surface ladder over drop shadows
- Logo tile: 64×64, 12px radius

## Components

- **TUI frame:** titlebar (`cox · session`), status line (`READY` + model chip), split panes
- **Prompt bar:** monospace input, forest elevated surface, chartreuse caret
- **Feature panes:** bordered TUI panels with mono headers (`01 · core`)
- **Agent chips:** module tags in accent-soft with mono 11–12px
- **Buttons:** solid forest (light) / chartreuse (dark); ghost = hairline
- **Status:** chartreuse for “steering / running”; muted for idle
- **Nav:** mono micro-labels; keep distinct from ketch’s mint wordmark style

## Mini landing wire

1. Hero: full-width TUI frame — titlebar + status + split panes (copy | session)
2. Feature panes: `01 · core` / `02 · safety` / `03 · cost`
3. CTA strip as deep terminal bar
4. Footer: Listepo + docs

## Do / Don't

**Do**
- Lean into forest depth + chartreuse spark
- Prefer mono + terminal chrome over glass marketing cards
- Keep mark as TUI pane + cursor (readable at favicon size)

**Don't**
- No bright teal/mint that collides with ketch
- No serif product chrome; no purple AI glow
- Don’t revive the helm wheel — mark is terminal TUI, not nautical
