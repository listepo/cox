# Desktop mockups

HTML mockups of the macOS client (`docs/design/desktop.md`, `../DESIGN.md`). One page holds every
screen; the URL hash picks it (`mockups.html#01-main-session-streaming`, `#__list` lists them).

Render screens to 2x PNGs (headless Chrome, 1520×980 window) into `screens/`, which is not committed:

```bash
./render.sh 28-main-glass-frosted 29-main-glass-glossy 30-main-glass-tokens
```

Screens 01–27 are the solid light/dark set; 28–30 are the glass main screen (frosted, glossy, token
popover). The CSS custom properties mirror `../tokens/`; once T37.17 lands, they come from
`../tokens/tokens.css`.
