# cox for macOS — design system

The one reference for how the desktop app looks and how its SwiftUI is built. It is written for two
readers: a person reviewing a screen and an agent generating one. If a screen needs something this file
does not have, add it here first (token, component or variant), then build the screen.

- **Source of truth for values:** `tokens/*.json` (W3C Design Tokens Format Module 2025.10). This file
  explains them; it never restates a value the JSON holds, except in the tables marked *mirror*.
- **Source of truth for looks:** `mockups/mockups.html`; `mockups/render.sh <screen-id>…` renders PNGs into `mockups/screens/` (not committed). The glass main screen
  is `28-main-glass-frosted`, `29-main-glass-glossy`, `30-main-glass-tokens`.
- **Behaviour and architecture:** `docs/design/desktop.md` (cited as DT§n). This file covers only the view layer.

## 1. Principles

1. **Views are dumb.** A SwiftUI view renders a value and reports intents. Business logic, formatting of
   numbers and text, and every decision about *what* to show live in Rust (`cox-app`) and reach Swift as
   view state through `CoxModel`. A view never calls FFI, never reads config, never opens a file.
2. **Tokens, never literals.** No colour, size, font, radius, shadow, duration or opacity is written as a
   number or literal in a view. Enforced by lint (§9).
3. **Decompose to reuse.** Every visual element is the smallest component that has a name in §6, built
   from the ones below it. Two screens that show the same thing use the same component; a difference is a
   *variant* parameter, never a copy.
4. **Depth has meaning.** Elevation says what is on top of what: prose is flat, cards are lifted, the
   composer floats highest in a pane, popovers float over panes, the window floats over the wallpaper.
   Never lift something to decorate it.
5. **Glass is for chrome, not for reading.** Transparency applies to the window and panes. Anything that
   carries text a person must read stays at least `material.readableFloor` opaque at every setting.
6. **The system wins.** Reduce Transparency forces Solid; Reduce Motion drops movement to cross-fades;
   Increase Contrast switches to high-contrast colour variants; the accent follows the system accent.

## 2. Token pipeline

```
design/tokens/color.light.json ┐
design/tokens/color.dark.json  ├─ Style Dictionary 5.x ─┬─ CoxUI/Tokens/Colors.xcassets  (colorsets: Any / Dark / High Contrast)
design/tokens/base.json        ┘                        ├─ CoxUI/Tokens/Tokens.swift     (Space, Radius, Size, Font, Elevation, Motion, Material)
                                                        └─ design/tokens/tokens.css     (the HTML mockups read the same values)
```

- Format: DTCG 2025.10 — `$type`/`$value`; colours as `{colorSpace, components, alpha, hex}`; dimensions
  as `{value, unit: "px"}` (1 px = 1 pt on macOS); shadows as arrays of layers with `inset`.
  Source: https://www.designtokens.org/tr/2025.10/format/ (checked 2026-09-28).
- Generator: Style Dictionary (npm `style-dictionary` 5.5.5, released 2026-09-20; checked in the npm
  registry 2026-09-28), pinned with its lockfile in `package.json`; `style-dictionary.config.mjs` holds
  the build. Built-in formats do not cover DTCG composite values or colorsets, so the Swift and CSS
  outputs are custom formats and the colorsets a custom action. `just desktop-tokens` runs it.
- Each `color.<mode>.json` holds every colour role for one appearance: `light` (Any) and `dark` are
  required; `light-hc` and `dark-hc` are optional and add the High Contrast entries. A token that also
  has children (`accent` and `accent.soft`) is the group's `$root` token (DTCG §6.2).
- Colours become asset-catalog colours; the build generates a `ColorResource` per colorset, so a view
  writes `Color(.accent)`, `Color(.surfaceWindow)` and so on (SwiftPM generates no `Color.<name>`
  extensions). Light, dark and high-contrast variants live in one colorset.
- `Tokens.swift` flattens each group's path to camelCase: `Space.m`, `Radius.pane`, `Size.readingWidth`,
  `Motion.durationFast`, `Motion.easingStandard` (a `UnitCurve`), `MaterialToken.frostedBlur`, and the
  values `FontToken.transcriptH3` and `ElevationToken.e2` that `.textStyle(_:)` and `.elevation(_:)`
  take. Font, Material and Elevation carry a `Token` suffix so they do not shadow SwiftUI's `Font` and
  `Material` or the `Elevation` modifier.
- A drift test (the `desktop-tokens` CI job) regenerates the outputs and fails on any diff, like
  `docs/config.jsonschema`. Edit the JSON, never a generated file.

## 3. Foundations

### 3.1 Colour — semantic roles

Name by role, never by hue. A view asks for `text.secondary`, not "grey".

| Group | Tokens | Use |
| --- | --- | --- |
| surface | `window`, `sidebar`, `code`, `capsule`, `capsuleBorder`, `popover`, `terminal` | Backgrounds, by layer |
| fill | `primary`, `secondary` | Quiet fills inside a surface |
| text | `primary`, `secondary`, `tertiary`, `terminal`, `terminalOk` | Foregrounds |
| line | `separator` | 0.5 pt hairlines |
| intent | `accent`, `accent.soft`, `status.success/warning/danger/plan` (+ `Soft`) | Meaning: selection, done, needs you, error, plan mode |
| diff | `add`, `addGutter`, `del`, `delGutter` | Diff lines and gutters |
| syntax | `keyword`, `string`, `number`, `function`, `comment`, `type` | Highlighting; same roles as `cox-render`'s `StyleToken`, so TUI and app match |
| data | `meter.sent`, `meter.received`, `context.system/tools/instructions/history` | Token meter and context bar |
| tile | `tile.<kind>.top/bottom/glyph` for `neutral`, `edit`, `shell`, `search`, `write` | Tool icon tiles |
| shadow | `shadow.tint` | The colour every elevation layer uses |

Mode colours: Ask uses `text.primary` on the selected segment, Plan uses `status.plan`, Auto uses
`accent`, Bypass fills the segment with `status.danger` and draws a 3 pt `status.danger` strip at the
window's top edge.

### 3.2 Typography

SF Pro Text for UI and SF Mono for code. Sizes are points at 100 % text size; the user scales all of
them together between 85 % and 150 % (⌘+ / ⌘−). Digits that change while you watch (cost, tokens,
tok/s, timers) are always tabular (`.monospacedDigit()`).

| Token | Use | *mirror* size / weight |
| --- | --- | --- |
| `font.title.window` | Session title in the toolbar | 13.5 / semibold |
| `font.body` | Default UI text | 13 / regular |
| `font.transcript` | Messages | 13.5 / regular, line height 1.55 |
| `font.transcript.h3` | Markdown headings | 15 / semibold |
| `font.control` | Buttons, capsules, segments | 12.5 / medium |
| `font.caption` | Thinking line, notices | 12 / regular |
| `font.footnote` | Tool status, meter, key–value rows | 11.5 / regular |
| `font.label` | Uppercase section headers, tracking 0.03 em | 11 / semibold |
| `font.micro` | Key caps, legends, badges | 10.5 / medium |
| `font.metric` | Big live numbers (tok/s) | 26 / semibold, tracking −0.02 em |
| `font.mono.code`, `mono.inline`, `mono.terminal` | Code, inline code, terminal tail | 12 / 12 / 11.5 |

### 3.3 Space, radius, size

- **Space** is a fixed scale: `xxs 2 · xs 4 · s 6 · m 8 · ml 10 · l 12 · xl 16 · xxl 20 · xxxl 24 · huge 32`.
  Inside a component use `xs`–`l`; between components use `m`–`xl`; between sections use `xxl`+.
- **Radius** goes up with the size of the thing: key cap `xs` → icon tile `s` → button `m` → tool card
  and row `l` → bubble `xl` → approval `xxl` → sidebar `panel` → pane and composer `pane` → popover
  `popover` → window `window`; pills use `capsule`. Nested shapes are concentric: inner radius = outer
  radius − inset.
- **Size** fixes the layout skeleton: toolbar 56, sidebar 252, inspector 324, reading column 760,
  gap between floating panes 8, capsule 32, button 28 / 24, icon tile 22, status dot 9, hairline 0.5.

### 3.4 Elevation (depth)

Six levels. Each is a stack of shadow layers tinted `shadow.tint` plus, from e1 up, a 1 pt inner
highlight on the top edge — the highlight is what makes glass read as a solid object.

| Level | What sits there |
| --- | --- |
| `e0` | Prose, collapsed tool rows, list rows at rest |
| `e1` | Chips, capsules, segmented controls, icon tiles, the selected row, key caps |
| `e2` | Cards: user bubble, expanded tool, approval, sidebar, inspector |
| `e3` | The composer — the highest thing inside a pane |
| `e4` | Popovers, menus, the command palette |
| `e5` | The window itself over the wallpaper |

The **Depth** setting (Flat … 3D) scales every level's shadow opacity and y-offset by 0…1; at Flat the
highlights go too and the app looks like a standard macOS app. Only `e5` ignores Depth.

### 3.5 Materials — glass

| Material | Window opacity | Blur | Specular | SwiftUI / AppKit |
| --- | --- | --- | --- | --- |
| Frosted | `material.frosted.windowOpacity` (default) | heavy | soft sweep | `glassEffect(.regular)`; window behind: `NSVisualEffectView`, `.behindWindow` |
| Glossy | lower | light | strong sweep and streak | `glassEffect(.clear)` + the `specular` overlay |
| Solid | 1 | none | none | Plain `surface.window`; forced by Reduce Transparency |

- The user sets material, window transparency, blur (frosted) or reflection (glossy), Depth, and
  "Tint from wallpaper" in the Appearance popover (toolbar paintbrush, ⌘⌥A) and in Settings ›
  Appearance. They are stored in Rust-owned config under `[desktop.appearance]`
  (`material`, `opacity`, `blur`, `depth`, `tint`), so they have a schema and provenance like any other
  setting.
- Text-bearing surfaces — messages, code, diffs, terminal, popovers, the composer — never drop below
  `material.readableFloor`. The transparency slider only moves the window and pane backgrounds.
- Panes are separate glass layers (sidebar, transcript column, inspector) with an 8 pt gap, so the
  wallpaper shows between them. Group neighbouring glass in one `GlassEffectContainer` so shapes blend
  and render in one pass.

### 3.6 Motion

- `motion.duration.fast` for hover and press, `base` for disclosure, `slow` for popovers and panes;
  easing `standard`, or `decelerate` for things entering.
- Streaming text is never animated per token. A new block fades in; a state change (spinner → ✓)
  cross-fades; the sparkline scrolls without easing.
- Reduce Motion: movement becomes a cross-fade of the same duration.

### 3.7 Icons

SF Symbols only, weight medium, rendering hierarchical; sizes 11, 12, 13, 14, 16. The mockup's icon
names map one-to-one:

| Mockup | Symbol | Mockup | Symbol |
| --- | --- | --- | --- |
| doc | `doc.text` | clip | `paperclip` |
| pencil | `pencil` | up | `arrow.up` |
| term | `terminal` | check | `checkmark` |
| search | `magnifyingglass` | chev / chevd | `chevron.right` / `chevron.down` |
| globe | `globe` | people | `person.2` |
| eye | `eye` | rewind | `arrow.uturn.backward` |
| paint | `paintbrush` | refresh | `arrow.clockwise` |
| insp | `sidebar.right` | comment | `text.bubble` |
| sparkle | `sparkle` | bolt | `bolt` |
| branch | `arrow.triangle.branch` | stop | `stop.fill` |

## 4. Layout

```
┌ window (e5, radius.window) ───────────────────────────────────────────────────────────┐
│ ┌ Sidebar (e2) ┐ ┌ Toolbar: Breadcrumb · spacer · ModelCapsule · ModeSegmented ·     ┐ │
│ │ SessionFilter│ │          CostCapsule · StopButton · AppearanceButton · Inspector  │ │
│ │ SectionHeader│ ├ TranscriptPane (glass) ──────────────┐ ┌ Inspector (e2) ──────────┤ │
│ │ SessionRow…  │ │   reading column 760, centred        │ │ InspectorTabs            │ │
│ │              │ │   Turn → blocks                      │ │ tab content              │ │
│ │ SidebarFooter│ │   Composer (e3) + TokenMeter         │ │                          │ │
│ └──────────────┘ └──────────────────────────────────────┘ └──────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- `NavigationSplitView` with three columns; sidebar and inspector are collapsible (⌘0, ⌘⌥0).
- The reading column is `size.readingWidth` wide and centred; the composer shares its width.
- Minimum window `size.windowMinWidth` × `size.windowMinHeight`. Below 1280 pt the inspector becomes
  an overlay instead of a column.

## 5. Swift package layout for the view layer

```
Packages/CoxUI/Sources/CoxUI/
├─ Tokens/        generated: Tokens.swift, Colors.xcassets — never edited by hand
├─ Foundations/   Appearance (the settings every modifier reads, with Reduce Transparency and Reduce
│                 Motion applied once) and the ViewModifiers and styles: Elevation, GlassPane,
│                 Specular, HairlineModifier, InsetWell, TextStyle, ControlState (rest, hovered,
│                 pressed, disabled for every button-like style), CoxButtonStyle, CapsuleStyle,
│                 CoxSegmented, CoxToggleStyle, CoxSlider, Knob
├─ Atoms/         one file per atom (§6.2)
├─ Molecules/     one file per molecule (§6.3)
├─ Organisms/     one file per organism (§6.4)
├─ Screens/       composition only: organisms + layout, no styling
└─ Previews/      PreviewState fixtures shared by every #Preview and snapshot test
```

A layer may use only the layers above it in this list. `Screens` contains no modifiers from
`Foundations`; if a screen needs styling, the styling belongs to a component. The Foundations
modifiers are `internal`, so nothing outside `CoxUI` can style a view; the app sets only the public
`coxAppearance` environment value from `[desktop.appearance]`.

## 6. Component catalogue

Every component: one file; takes a value-type state (`Equatable`, `Sendable`) and closures or an intent
enum; has a `#Preview` for each variant in light, dark, Solid and Frosted; has a snapshot test. The
"CSS" column names the mockup class, so an agent can translate a mockup element straight to its
component.

### 6.1 Foundations (modifiers and styles)

| Name | What it does | Tokens | CSS |
| --- | --- | --- | --- |
| `.elevation(_ level:, cornerRadius:)` | Shadow layers + top highlight, scaled by Depth | `elevation.e0–e5`, Depth | `--lift1…3` |
| `.glassPane(_ shape:, surface:, role:)` | Pane material: glass or solid per setting; `role: .readable` holds the readable floor | `material.*`, `surface.*` | `.glass .col`, `.sidebar`, `.insp` |
| `.specular(_ strength:, in:)` | Diagonal highlight overlay (strong for Glossy, faint for Frosted, none for Solid) | `material.*.specular` | `.window:after` |
| `.hairline(_ edges:)`, `.hairline(in:)` | 0.5 pt `separator` line on edges or around a shape | `size.hairline`, `separator` | `border:.5px` |
| `.insetWell(_ surface:, cornerRadius:)` | Pressed-in look for terminal and fields | `surface.terminal`, inner shadow | `.tail`, `.filter` |
| `.textStyle(_ token:, tabularDigits:)` | Font at the text size, line height, tracking, tabular digits | `font.*` | font rules |
| `.symbolStyle(_ token:)` | An SF Symbol at a font token's size, weight medium, rendered hierarchical (§3.7) | `font.*` | `svg` icons |
| `CoxButtonStyle(.primary/.secondary/.danger/.plain, size: .regular/.small)` | All push buttons: e1 face with specular; hover tints, press sinks to e0; disabled keeps the readable floor and a `text.secondary` label | `size.button*`, `radius.m`, `font.control`, `fill.*` | `.pb`, `.pri`, `.dan` |
| `CapsuleStyle(.plain/.active)` | Toolbar capsules and filter chips: readable glass face at e1 with a hairline; active takes `surface.window`, an `accent` label and an `accent.soft` halo; states as `CoxButtonStyle` | `size.capsuleHeight`, `radius.capsule`, `surface.capsule`, `font.control` | `.cap`, `.cap.hot` |
| `CoxSegmented(_ label:, selection:, options:, look:, title:)` | Segmented control; the e1-lifted selection pill slides between segments, or cross-fades under Reduce Motion (`coxMatchedGeometry`). `look` marks the selected segment per option: `plain`, `tinted(colour)` label, or `filled(colour)` pill with a white label (the §3.1 mode colours). A view, not a `PickerStyle`: SwiftUI has no public hook to restyle segments on macOS | `e1`, `surface.capsule`, `font.control` | `.seg` |
| `CoxToggleStyle`, `CoxSlider(_ label:, value:, in:)` | Toggles and sliders with the shared 3D `Knob` (white disc, hairline rim, e1) over an `insetWell` track filled with `accent`; the toggle's knob slides, or cross-fades under Reduce Motion. The slider is a view: macOS has no public `SliderStyle` | `e1`, `accent`, `fill.secondary` | `.tog`, `.slider` |

### 6.2 Atoms

| Atom | Variants / states | CSS |
| --- | --- | --- |
| `StatusDot` | running, waiting, idle, error | `.dot .d-*` |
| `IconTile(kind, symbol)` | neutral, edit, shell, search, write; the tool picks the DS§3.7 symbol | `.tool .ic.c-*` |
| `KeyCap` | — | `.kbd` |
| `Badge` | neutral, user, project, env, default, warning, danger | `.badge .b-*` |
| `CountBadge` | — | `.sect .cnt` |
| `RiskChip(text, level)` | low, medium, high — drawn as a `Badge`: neutral, warning, danger | `.risk` |
| `Spinner`, `ProgressRing(fraction)` | — | `.spin`, `.ring` |
| `Sparkline(samples)` | tint | `svg` in `.meter` |
| `StackedBar(segments)` | — | `.tokpop .bar` |
| `DiffStat(added, removed)` | — | `.plus`, `.minus` |
| `SectionHeader(title, trailing)` | — | `.sect`, `.ih` |
| `InlineCode`, `Hairline(orientation)` | `Hairline`: horizontal, vertical; drawn by `.hairline` | `code`, `.divider:before`, `.sep` |
| `Thumbnail(attachment)` | image, file | `.thumb` |

### 6.3 Molecules

| Molecule | Built from | CSS |
| --- | --- | --- |
| `SessionRow(item, isSelected:)` | StatusDot, title, subtitle, cost; selected on `accent.soft` at e1 | `.row` |
| `SessionFilter(text:, prompt:, shortcut:)` | search field in an `insetWell`, KeyCap | `.filter` |
| `Breadcrumb(title, project:, branch:)` | title, project, branch | `.crumb` |
| `ModelCapsule(model, isOpen:)`, `CostCapsule(cost:, context:, fraction:, isOpen:)` | CapsuleStyle (active while open), ProgressRing | `.cap` |
| `ModeSegmented(selection:)` | CoxSegmented; ask, plan, auto, bypass (offered only while on) | `.seg` |
| `StopButton` | KeyCap; inverted `text.primary` capsule answering ⌘. | `.stop` |
| `ToolHeader` | IconTile, summary, RiskChip, status, disclosure | `.tool .h` |
| `DiffLineView`, `DiffHunkView` | gutter, syntax runs | `.diff .ln`, `.hh` |
| `CodeBlockView` | header, copy button, highlighted runs | `.codeblock` |
| `TerminalTail` | insetWell, lines | `.tail` |
| `UserBubble(text, attachments:)` | prompt in `font.transcript`, a row of Thumbnail; readable face at e2, the glass sweep behind the text | `.user`, `.user .att` |
| `ThinkingDisclosure(summary, text:, isExpanded:)` | chevron and caption summary; open, the reasoning in italic caption beside a hairline; open state is the view's own | `.think`, `.think-body` |
| `NoticeRow`, `TurnDivider`, `TurnMeta` | icon, caption | `.notice`, `.divider`, `.meta` |
| `ComposerChip` | icon, label, KeyCap | `.chip` |
| `TokenMeter` | ↑ sent, ↓ received, StatusDot, tok/s, Sparkline | `.meter` |
| `KeyValueGrid(columns:, rows:)` | rows of label / values under optional column headers; detail rows indented in `text.secondary` | `.tokpop .grid` |
| `MaterialPicker` | three swatches | `.mat` |
| `LabeledSlider(title, value:, in:, valueText:, ends:)`, `LabeledToggle(title, detail:, isOn:)` | SectionHeader + CoxSlider + end labels / CoxToggleStyle with an optional detail line | `.appear .lbl`, `.row2` |
| `ChangedFileRow`, `CheckpointRow` | icon, path, DiffStat / time | inspector rows |

### 6.4 Organisms

| Organism | Built from | CSS |
| --- | --- | --- |
| `Sidebar` | SessionFilter, SectionHeader, SessionRow, SidebarFooter | `.sidebar` |
| `SessionToolbar` | Breadcrumb, ModelCapsule, ModeSegmented, CostCapsule, StopButton | `.toolbar` |
| `ToolCard` | ToolHeader + one body: DiffHunkView, TerminalTail, CodeBlockView | `.tool`, `.tool.exp` |
| `ApprovalCard` | header, command well, reasons, CoxButtonStyle row | `.appr` |
| `AssistantMessage` | markdown runs, InlineCode, CodeBlockView | `.asst` |
| `TurnView` | UserBubble, ThinkingDisclosure, ToolCard, AssistantMessage, TurnMeta | `.turn` |
| `TranscriptView` | lazy list of TurnView from timeline patches | `.scroll` |
| `Composer` | text field, ComposerChip, TokenMeter, send button | `.composer` |
| `TokenPopover` | metric, Sparkline, KeyValueGrid, StackedBar, legend | `.tokpop` |
| `AppearancePopover` | MaterialPicker, LabeledSlider ×3, LabeledToggle ×2 | `.appear` |
| `Inspector` | tabs + ChangedFileRow, CheckpointRow, KeyValueGrid | `.insp` |

### 6.5 The glass main screen, decomposed

`MainScreen` = `Sidebar` + `SessionToolbar` + `TranscriptView` + `Composer` + `Inspector`, with
`AppearancePopover` or `TokenPopover` as popovers. It holds no styling of its own.

## 7. Data shown in the token meter

- ↑ **sent** = input + cache read + cache write; ↓ **received** = output, including thinking. Session
  totals come from the cost ledger (one `usage` row per request); per-turn values are the same rows
  filtered by turn.
- **tok/s** during streaming is estimated in `cox-app` from output deltas (`cox-tokens` estimate over a
  rolling window) and replaced by the exact figure when the request's usage arrives. The UI never
  computes it.
- The context bar reuses the context breakdown the TUI already shows (P28): system, tools, instruction
  files, history.

## 8. Accessibility

- Every interactive element has a label; icon-only buttons have a tooltip with their shortcut.
- Text contrast is at least 4.5:1 against its surface in every material and appearance; the snapshot
  suite checks the Frosted renders.
- VoiceOver rotors: Approvals, Tool calls, Errors. The token meter reads as "218 thousand tokens sent,
  9.8 thousand received, 71 tokens per second".
- Honour Reduce Transparency, Reduce Motion and Increase Contrast (§1.6).

## 9. Rules for agents generating SwiftUI

Before writing a view:

1. Find the element in §6 (search by the mockup's CSS class). If it exists, use it. If it nearly
   exists, add a variant to it. Only if nothing fits, add a new row to §6 in the same change.
2. Build bottom-up: atoms, then molecules, then organisms. A screen file only composes.

While writing:

- Use generated tokens only: `Color(.<role>)`, `Space.<step>`, `Radius.<step>`, `Size.<name>`,
  `.textStyle(.<token>)`, `.elevation(.<level>)`, `Motion.<token>`. No `Color(red:…)`, `.padding(12)`,
  `.font(.system(size:…))`, `.shadow(…)`, `.cornerRadius(…)` or `withAnimation(.easeIn(duration:…))`
  with literals.
- Subviews are `struct`s, not computed properties or `@ViewBuilder` functions, so SwiftUI can skip them.
  Keep `body` short; extract when a view does two things.
- Inputs are value types from `CoxModel`; a component never imports `CoxCore` (the FFI) and never owns
  business state. Local UI state (hover, disclosure open) is `@State` inside the component.
- Lists use stable ids from Rust; row state is `Equatable`.
- One component per file, named after the component.

Before finishing:

- `#Preview` for every variant × light/dark × Solid/Frosted, using `Previews/PreviewState`.
- Snapshot tests (swift-snapshot-testing 1.19.6, checked on GitHub 2026-09-28) for the same matrix.
  A missing reference is recorded and fails once; `SNAPSHOT_TESTING_RECORD=all swift test` re-records
  after an intended change, and a second run must pass.
- SwiftLint (0.65.1, checked on GitHub 2026-09-28) passes, including the custom rules that reject
  literal colours, sizes, fonts, radii, shadows and durations outside `Tokens/` and `Foundations/`.
- If you added or changed a token, the drift test passes and `tokens/tokens.css` is regenerated.
