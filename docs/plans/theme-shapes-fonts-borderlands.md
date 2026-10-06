# Theme shapes, fonts and Borderlands menus

Status: proposed. This plan changes no runtime behaviour.

## Visual target

The supplied [Borderlands menu screenshot](https://www.gameuidatabase.com/uploads/Borderlands06102021-092844-14510.jpg)
shows translucent dark blue surfaces, cyan edging, pale blue-white text and a
yellow selected row with dark text and a diagonal right edge. Its menu labels
are squared and condensed; its heading is broader and heavier.

Use that menu as the reference. The current grey salvage-panel treatment misses
both its blue interface and its typography. Do not infer the menu font from the
game's logo. The exact typefaces in this screenshot remain unidentified.

## 1. Correct the Borderlands palette

- Replace grey surfaces with muted navy, blue steel and cyan edges. Keep the
  ordinary keys readable and visually quieter than controller highlights.
- Give selected menu rows yellow fill and dark text. Keep keyboard selections
  blue, purple and orange for rare, epic and legendary loot; hover remains green.
  Menu focus and weapon-rarity highlights serve different purposes.
- Replace the salvage-panel artwork with restrained blue interface decoration.
  Any scan lines must stay faint beneath text. Keep the status strip clear.
- Review the keyboard, Settings, theme/layout pickers, mappings, notifications
  and battery/controller labels together. Check contrast after alpha blending.

This stage uses existing theme fields. Update `themes/borderlands.toml`, its
artwork and `docs/themes.md`; preserve the packaged asset paths or update the
packaging script with them.

## 2. Add parameterised corner styles

Introduce one reusable surface-shape resolver for filling, outlining and
containment. Shapes belong to theme data, never to a theme-name condition.
Select a corner style, then configure its parameters. Start with `square`
(no parameters), `rounded` (corner radii) and `beveled` (straight diagonal cuts).
Defer the screenshot's extra stepped header/footer silhouette: it changes the
whole outline and belongs outside corner styling. Future corner styles can
introduce their own parameters.

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

## 3. Make fonts configurable per theme

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

Watch font files with other theme assets. Load off the drawing thread; apply
new definitions only when the effective font set changes, using egui's
[font update API](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.set_fonts).
Discard stale loads, report missing/corrupt files once, and use the default font
for the affected role. Switching back to Default restores its typography.

Invalidate font-dependent label/layout caches when roles change. In particular,
`keyboard/display_icon.rs::LabelCache` currently keys widget text by template,
size and colour, without font identity. Re-measure menu rows, suggestions,
keyboard labels and window bounds; verify long labels and mixed text/icon keys.

Trial Rajdhani SemiBold for menus and Oxanium Bold for headings as stylistic
approximations. Both are available under the SIL Open Font License:
[Rajdhani licence](https://raw.githubusercontent.com/google/fonts/main/ofl/rajdhani/OFL.txt),
[Oxanium licence](https://raw.githubusercontent.com/google/fonts/main/ofl/oxanium/OFL.txt).
Confirm a static heading face supported by the decoder before packaging it.
Retain upstream sources, hashes, copyright and licence files; include the chosen
fonts with release assets. Compare typography at KOSK's actual window size.

## Delivery and acceptance

Deliver focused commits in this order: blue/yellow palette; shared menu geometry;
native window cuts; theme font service; Borderlands font assets and visual tuning.

- Existing themes render identically when new options are absent.
- Corner-style tests cover parameter validation, per-corner overrides and legacy
  radius compatibility. Geometry tests cover asymmetric/zero cuts, small
  rectangles, border insets, containment and default-boundary restoration.
- Font tests cover fallback, missing/corrupt files, role isolation, switching,
  replacement reloads, stale loads, glyph fallback and cache invalidation.
- Use generated/inline fixtures; automated tests must not depend on mutable
  shipped themes. For concrete bug fixes, observe the regression failing first.
- After Rust edits run `cargo fmt`, targeted library tests and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- Verify native corners and click behaviour on Windows at multiple DPI scales,
  in opaque and transparent modes, with both supported renderers.
- Compare live keyboard and menu captures with the supplied reference. Check
  yellow menu focus, distinct rarity colours, text contrast, icons, long labels
  and the clear status strip. Verify packaged font/artwork paths.

Background images may supply texture; window and selection shapes come from
geometry. Per-key illustrated skins and full cel-shading are separate work.
