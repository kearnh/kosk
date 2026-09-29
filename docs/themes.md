# Themes

Add named theme files to `config.toml`:

```toml
active_theme = "Amber"

[themes]
Amber = "themes/amber.toml"
```

Relative paths start beside the active config file. Absolute paths also work.
`default` is reserved for the built-in appearance. Custom names must be nonempty.
Every listed file must be readable and valid, including inactive themes.

Open **Settings → Themes**. **Default** comes first; custom names follow in
sorted order. Use up/down to highlight a theme, then confirm to apply and save
it. Clicking a theme also selects it. The active theme is marked **Current**.
A config supplied on the command line keeps the existing live-only settings
behavior. The built-in theme's config name remains `default`.

Theme files reload after edits or replacement. An invalid reload keeps the
previous appearance and displays a notice. Invalid startup configuration uses
the built-in defaults. Recordings use the current local theme.

## Example

The repository includes an [Old Steam Controller theme](../themes/old-steam-controller.toml):
navy surfaces, square blue keys, green Return/Done keys, and green selection accents. Register its path
under `[themes]` to make it available in Settings.

Save this as `themes/amber.toml` beside the config file:

```toml
background_color = [24, 20, 16, 255]
text_color = [245, 230, 210, 255]
selection_background_color = [125, 80, 30, 255]

[inactive]
corner_radius = 8.0

[keyboard.inactive]
background_color = [70, 52, 32, 180]
weak_background_color = [70, 52, 32, 180]
text_color = [255, 240, 215, 255]
corner_radius = 8.0

[keyboard]
left_selection_color = [175, 100, 30, 255]
right_selection_color = [100, 130, 45, 255]
dual_selection_color = [150, 85, 110, 255]

[suggestions]
background_color = [70, 52, 32, 175]
selected_background_color = [175, 100, 30, 215]
corner_radius = 8.0
selected_outline_width = 1.5

[text_input]
background_color = [35, 28, 20, 255]
text_color = [255, 240, 215, 255]
cursor_color = [255, 200, 100, 255]
```

Omitted fields inherit the built-in theme. Colors are `[red, green, blue, alpha]`,
with four integers from 0 to 255; alpha 0 is transparent. Border widths are
finite, nonnegative numbers. Corner radii range from 0 to 255 and round to whole
points. Unknown sections or fields are errors.

## Fields

- Top level: `background_color`, `text_color`, `muted_text_color`,
  `selection_background_color`, `selection_border_color`,
  `selection_border_width`, `window_border_color`, `window_border_width`,
  `window_corner_radius`.
- `[noninteractive]`, `[inactive]`, `[hovered]`, `[active]`, `[open]`: `background_color`,
  `weak_background_color`, `text_color`, `border_color`, `border_width`,
  `corner_radius`. These style standard controls, including both pickers.
  Top-level `text_color` supplies their text color unless a control state overrides it.
- `[keyboard.inactive]`, `[keyboard.hovered]`, `[keyboard.active]`: the same
  control fields, applied to keyboard keys.
- `[keyboard]`: `selection_background_color`, `selection_text_color`,
  `left_selection_color`, `right_selection_color`, `dual_selection_color`,
  `modifier_text_color`.
- `[[keyboard.key_groups]]`: `keys`, `background_color`, `text_color`.
- `[suggestions]`: `background_color`, `text_color`,
  `selected_background_color`, `selected_text_color`, `empty_slot_background`,
  `armed_color`, `disarmed_color`, `new_word_mark_color`, `corner_radius`,
  `selected_outline_width`.
- `[text_input]`: `background_color`, `text_color`, `cursor_color`.
- `[menus]`: `heading_color`, `muted_text_color`.
- `[mappings]`: `row_focus_color`, `table_focus_color`, `focus_border_color`,
  `focus_border_width`, `text_color`, `warning_color`, `error_color`,
  `success_color`, `unsaved_color`, `editor_background_color`,
  `editor_border_color`, `editor_border_width`, `editor_corner_radius`.
- `[move_window]`: `background_color`, `border_color`, `border_width`,
  `corner_radius`, `text_color`, `text_shadow_color`.
- `[notifications]`: `background_color`, `border_color`, `border_width`,
  `corner_radius`, `muted_text_color`, `info_color`, `warning_color`, `error_color`.
- `[battery]`: `empty`, `low`, `medium`, `high`, `full`, `charging`, `unknown`.

The existing keyboard/menu opacity settings multiply the theme background
alpha while the window is transparent. An opaque window uses an opaque
background. Move mode keeps its transparent window and themed ghost outline.

## Key groups

Repeat this table to color several sets of keys:

```toml
[[keyboard.key_groups]]
keys = ["Return", "exit"]
background_color = [63, 100, 58, 255]

[[keyboard.key_groups]]
keys = ["q", "w", "e", "r", "t"]
background_color = [125, 60, 65, 255]
text_color = [235, 220, 220, 255]
```

`keys` uses the layout's key syntax: characters, Enigo names such as `Return`,
keyboard actions such as `exit`, or literal text. Match the normal key, even
when Shift changes its output. Labels and icons do not affect matching.
Keys absent from the layout have no effect.

Each group needs a nonempty key list, nonempty key strings, and at least one
RGBA color. Space (`" "`) is valid. Later groups override earlier groups per
color; omitted colors preserve earlier values. Unmatched keys keep the ordinary
keyboard theme. Selection fills override group backgrounds; layout display-rule
colors retain highest priority.

## Overrides

Explicit colors in the config's `[text_input]`, `[completion.ui]`, and `[battery]`
sections override the theme. Explicit `completion.ui.corner_radius` and
`completion.ui.selected_outline_width` values also override it. Remove an
override to inherit the theme. Adjusting either shape setting in Settings
creates an explicit override.

Layout display-rule colors retain priority over themed key colors and selection
fills. Explicit battery `charging`, `low`, and `empty` colors also retain their
existing priority for notification accents. Font sizes, spacing, layout geometry,
and diagnostic overlays use their existing configuration.
