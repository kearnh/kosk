# Theme shapes and fonts

Status: implemented.

Added shared square, rounded and beveled geometry, native Windows clipping,
and independent font roles. Existing themes retain their geometry and fonts
when these options are absent. Font files are cached without watching.

Validation: 590 library tests pass; four remain ignored. Strict workspace Clippy
passes. Live captures cover the keyboard, Settings, pickers, mappings and move
mode. Native hit tests confirm cut corners pass through in transparent and
opaque windows with Glow and WGPU. Default and move mode restore rectangular
boundaries. Live controller checks also confirm hiding and showing preserves
the region. Native adapter tests cover 1.0, 1.25 and 2.0 scale factors; live
testing used the current monitor scale, not multiple physical monitor DPIs.

The extra `show_hide` integration test fails before exercising this change:
its unchanged fixture uses unsupported `config_version = 1`. The live
visibility check uses version 3.

## Parameterised corner styles

Introduce one reusable surface-shape resolver for filling, outlining and
containment. Shapes belong to theme data, never to a theme-name condition.
Select a corner style, then configure its parameters. Start with `square`
(no parameters), `rounded` (corner radii) and `beveled` (straight diagonal cuts).

Borrow CSS's separation of corner shape and size. Its
[corner-shape specification](https://www.w3.org/TR/css-borders-4/#corner-shaping)
defines `bevel` as a straight diagonal and `round` as a curved corner. Use
descriptive TOML settings rather than implementing CSS syntax, a cascade or a
general-purpose shape language. Defer scoops, notches and superellipses.

Proposed theme additions, in logical UI points:

```toml
[window]
corner_style = "beveled"

[window.beveled_corners]
corner_size = 12.0

[menus.selection]
corner_style = "beveled"

[menus.selection.beveled_corners]
corner_size = 0.0
top_right = 8.0
```

`corner_size` sets the distance along each adjoining edge; the example cuts all
window corners by 12 points and only the menu row's top-right corner by 8 points.
Optional `top_left`, `top_right`, `bottom_right` and `bottom_left` values override
individual corners. Zero leaves that corner square.

For `rounded`, use a `[window.rounded_corners]` block with `corner_radius` and the
same per-corner overrides. Reuse this corner-style schema for menu selections
and later surfaces. Window geometry and menu geometry remain independently configurable;
fill colours, typography and opacity remain separate theme settings.

Omitting a surface's `corner_style` and parameter blocks preserves its existing
radius behaviour. An explicit corner style replaces that legacy geometry; do
not combine rounding and cuts. Corner sizes default to zero, with omitted
corner overrides inheriting that size. Reject unknown corner styles,
mismatched parameter blocks and negative/nonfinite values. Clamp valid sizes
against the available rectangle
so small windows cannot produce inverted polygons. Derive border insets from
geometry so stroke widths remain consistent at diagonal corners.

Use the same primitive for the window outline and selected menu rows, including
theme/layout pickers through `src/ui/menu_list.rs`. Other surfaces can adopt it
later without introducing another geometry implementation. Reserve sufficient
content padding so cuts cannot clip labels or the bottom status strip.

### Actual window boundary

A polygon painted over a rectangular framebuffer is insufficient: `main.rs`
currently clears the whole window with `visuals.panel_fill`. The discarded
corners would still be coloured and could still receive input.

Use a private native-window adapter to apply a matching Windows polygon region
with [SetWindowRgn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowrgn).
Keep Win32 handles and resource ownership inside that adapter; application code
supplies resolved shape data. Do not expose the private theme/UI modules.
Review any required visibility changes before implementation.

- Convert logical geometry to physical pixels using current DPI and actual
  window bounds; region coordinates are window-relative, not client-relative.
- Refresh only when shape, size or DPI changes. Account for asynchronous resize
  commands and mode changes. Remove the region when returning to an uncut theme.
- Transfer region ownership only after a successful native call; free it on
  failure. Report failure and restore a consistent rectangular fallback.
- Match the native boundary to the painted outline. The region clips the clear
  colour, background images and controls, including in opaque mode.
- Verify cut corners expose the underlying application and do not intercept its
  clicks. Preserve show/hide, tray, move mode and text-entry focus behaviour.

## Theme font roles

Current fonts are installed once in `KoskApp::new`. Menu rows explicitly use
`FontId::proportional`; keyboard labels cache their font-dependent widget text.
Changing only the default egui font would not provide independent typography.

Add optional font roles, with paths resolved beside the theme file:

```toml
[fonts]
body_font = "fonts/Rajdhani-Medium.ttf"
menu_font = "fonts/Rajdhani-SemiBold.ttf"
heading_font = "fonts/Oxanium-Bold.ttf"
keyboard_font = "fonts/Rajdhani-Medium.ttf"
```

These filenames illustrate the proposed schema; they are not installed assets.
Omitted roles retain current fonts. Start with verified TTF support. Select
weight through the font file. Preserve existing layout/key font-size precedence;
add optional menu/heading size overrides where fixed sizing prevents a good fit.

Implement a font service that owns bounded loading, validation, fallback chains,
role resolution and egui registration. Widgets request a role and size, never
read font files. Preserve Phosphor icon families, Windows symbol/emoji fallbacks
and the editor's monospace font. Missing glyphs fall back to current fonts.

Load fonts on first use and cache them for the session; do not watch font files.
Switching themes or changing configured font paths resolves the required fonts.
Replacing a font file at the same path requires restarting KOSK.
Load off the drawing thread; apply new definitions only when the effective font
set changes, using egui's
[font update API](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.set_fonts).
Discard stale loads, report missing/corrupt files once, and use the default font
for the affected role. Switching back to Default restores its typography.

Invalidate font-dependent label/layout caches when roles change. In particular,
`keyboard/display_icon.rs::LabelCache` currently keys widget text by template,
size and colour, without font identity. Re-measure menu rows, suggestions,
keyboard labels and window bounds; verify long labels and mixed text/icon keys.

## Delivery and acceptance

- Existing themes render identically when new options are absent.
- Corner-style tests cover parameter validation, per-corner overrides and legacy
  radius compatibility. Geometry tests cover asymmetric/zero cuts, small
  rectangles, border insets, containment and default-boundary restoration.
- Font tests cover fallback, missing/corrupt files, role isolation, switching,
  configured path changes, session caching, stale loads, glyph fallback and
  cache invalidation.
- Use generated/inline fixtures; automated tests must not depend on mutable
  shipped themes. For concrete bug fixes, observe the regression failing first.
- After Rust edits run `cargo fmt`, targeted library tests and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- Verify native corners and click behaviour on Windows at multiple DPI scales,
  in opaque and transparent modes, with both supported renderers.

Background images may supply texture; window and selection shapes come from
geometry. Per-key illustrated skins and full cel-shading are separate work.
