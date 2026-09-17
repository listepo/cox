# cox — Design System

## Overview

**cox** is a modular terminal coding agent — the coxswain that steers. Metaphor: helm / tiller / steady hand. Evolves the existing editorial green/serif identity toward calm nerd mono + dual theme, while staying distinct from ketch mint (cox is deeper forest + chartreuse, not bright teal-mint).

Identity: deep forest ink + chartreuse mint accents. Helm mark signals control without nautical kitsch.

## Colors

### Light

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

### Dark

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

- **Mono (UI, terminal, labels):** IBM Plex Mono (fallback JetBrains Mono)
- **Sans (marketing body):** IBM Plex Sans — replaces prior serif editorial for product UI; serif may remain for long-form blog only
- Scale: 12 / 14 / 16 / 20 / 28 / 40
- Weights: 400 body, 500 labels, 600 headings

## Layout

- Terminal-first: prefer fixed-width mono columns for agent panes
- Max marketing width: 880px
- 8px grid; hairline borders; surface ladder over drop shadows
- Logo tile: 64×64, 12px radius

## Components

- **Prompt bar:** monospace input, forest elevated surface, chartreuse caret
- **Agent chips:** module tags in accent-soft with mono 11–12px
- **Buttons:** solid forest (light) / chartreuse (dark); ghost = hairline
- **Status:** chartreuse for “steering / running”; muted for idle
- **Nav:** mono micro-labels; keep distinct from ketch’s mint wordmark style

## Mini landing wire

1. Hero: helm mark + `cox` wordmark + “Steady hand for terminal coding.”
2. Terminal mock: agent session snippet
3. Modules strip: composable tools as mono chips
4. Footer: Listepo + docs

## Do / Don't

**Do**
- Lean into forest depth + chartreuse spark
- Use mono for agent UI; keep calm, editorial restraint
- Preserve distinctiveness vs ketch (deeper, less minty, helm not sail)

**Don't**
- No bright teal/mint that collides with ketch
- No serif-only product chrome; no purple AI glow
- Don’t simplify the helm to a generic circle — keep spokes readable at favicon size
