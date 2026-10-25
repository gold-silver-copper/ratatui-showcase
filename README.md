# ratatui-showcase

A fullscreen [ratatui](https://ratatui.rs) demo screen for big monitors and
projectors. The ratatui logo and the "ratatui" wordmark take turns, scaled to
fill any terminal while keeping their proportions, with
[tachyonfx](https://github.com/ratatui/tachyonfx) transitions in the style of
[Omarchy](https://omarchy.org)'s screensaver.

```sh
cargo run --release
```

| Key | Action |
| --- | --- |
| `Space`, `→`, `Enter` | Next transition |
| `t` | Next colour theme |
| `s` | Next shader |
| `i` | Show or hide the caption |
| `q`, `Esc`, `Ctrl-C` | Quit |

## What it shows

- **Logos:** the ratatui mascot, and the pixel-art rat from `assets/logo.svg`.
- **Wordmarks:** the `RatatuiLogo` small and tiny block lettering, the classic
  8x8 bitmap font, and the word set in a random font installed on the system.

Each scene gets a colour theme (Classic, Aurora, Synthwave, Ember, Ocean,
Candy, Phosphor, Prism) and a shader (flow, plasma, pulse, sheen, rainbow,
scanline), with a soft glow around the art.

## How it scales

Everything is drawn as pixels, two per cell with `▀`. Pixel art scales by
whole numbers so every pixel stays the same size; fonts are rasterized at the
exact size that fits. The terminal's pixel size, where it reports one, corrects
for the shape of its cells, so the art is never stretched.

## Credits

The mascot and the `RatatuiLogo` lettering come from `ratatui-widgets`, and the
rat from ratatui's `assets/logo.svg`, both MIT licensed by the ratatui
developers.
