# Themes

List theme files or glob patterns in `config.toml`:

```toml
active_theme = "Amber"
themes = ["themes/*.toml", "../shared-themes/*.toml"]
```

Relative paths start beside the active config file. Absolute paths and `*`, `?`,
and bracket patterns are supported. Each theme file sets its display name with
`name = "Amber"`. Names are mandatory, unique and nonempty; `default` is
reserved for the built-in appearance. Invalid files are skipped and reported
in one notification with a limited file list.

Open **Settings → Themes**. **Default** comes first; custom names follow in
sorted order. Use up/down to highlight a theme, then confirm to apply and save
it. Clicking a theme also selects it. The active theme is marked **Current**.
A config supplied on the command line keeps the existing live-only settings
behavior. The built-in theme's config name remains `default`.

Theme files reload after edits or replacement. Invalid files are skipped;
an unavailable active theme falls back to Default and displays a notice.
Invalid startup configuration uses the built-in defaults. Recordings use the
current local theme.

## Included themes

The repository includes [Old Steam Controller](../themes/old-steam-controller.toml),
[Portal 2](../themes/portal.toml), and [Factorio](../themes/factorio.toml) themes.
Add `themes/*.toml` to the `themes` list to discover them.

The Portal 2 theme uses clean gray-white surfaces with blue and orange accents.

The Factorio theme uses charcoal and steel panels with amber and copper accents.

## Example

Save this as `themes/amber.toml` beside the config file:

```toml
name = "Amber"
background_color = [24, 20, 16, 255]
keyboard_opacity = 0.8
ui_opacity = 1.0
text_color = "cream"
selection_background_color = [125, 80, 30, 255]

[colours]
cream = [245, 230, 210]
key_background = [70, 52, 32, 180]

[inactive]
corner_radius = 8.0

[keyboard.inactive]
background_color = "key_background"
weak_background_color = "key_background"
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

Omitted fields inherit the built-in theme. Every colour field accepts
`[red, green, blue]`, `[red, green, blue, alpha]`, or a quoted name from
`[colours]` (`[colors]` is an alias). Channels are integers from 0 to 255;
omitted alpha is 255, and alpha 0 is transparent. Palette entries use RGB or
RGBA tuples. Use only one spelling of the palette table per file. Undefined
names and invalid tuples make the theme invalid. Border widths are
finite, nonnegative numbers. Corner radii range from 0 to 255 and round to whole
points. Unknown sections or fields are errors.

## Control states

These states describe individual controls, independently of window focus:

- `inactive`: resting buttons and controls.
- `hovered`: controls under the mouse pointer.
- `active`: controls being pressed or dragged; egui also uses this for widget focus.
- `open`: expanded controls, such as an open dropdown.
- `noninteractive`: labels and other display-only elements.

Keyboard keys use `keyboard.inactive`, `keyboard.hovered`, and `keyboard.active`
for ordinary mouse interaction. Controller selection uses the separate
`left_selection_color`, `right_selection_color`, and `dual_selection_color`.
Key press pulses use `key_press_color`. Explicit key fills, including selection,
key groups and pulses, override the state background.

`open` has no current caller in kosk. Keyboard labels use explicit colours,
so `keyboard.hovered.text_color` and `keyboard.active.text_color` have no effect.
Their backgrounds and borders still apply. All state sections are optional;
keep only the overrides your theme needs.

## Fields

- Top level: `background_color`, `keyboard_opacity`, `ui_opacity`, `text_color`, `muted_text_color`,
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
  `modifier_text_color`, `key_press_color`, `key_press_duration_ms`.
- `[[keyboard.key_groups]]`: `keys`, `background_color`, `text_color`.
- `[suggestions]`: `background_color`, `text_color`,
  `selected_background_color`, `selected_text_color`, `empty_slot_background`,
  `armed_color`, `disarmed_color`, `new_word_mark_color`, `corner_radius`,
  `selected_outline_width`.
- `[text_input]`: `background_color`, `text_color`, `cursor_color`.
- `[stick_pad_cursors]`: `radius`, `appearance` (`solid`, `fade`, `ring`),
  `opacity`, `ring_thickness`, `ring_fill_opacity`, `left_color`, `right_color`.
  Supplied values override config; omitted values use config defaults. Colors accept palette
  names or RGB/RGBA arrays. Cursor visibility stays in config.
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

Key presses flash `key_press_color` over the key's background and fade out.
Defaults are `[255, 255, 255, 180]` and `key_press_duration_ms = 180`.
Set the duration to `0` to disable animation; allowed durations are `0`–`65535` ms.
Repeated presses restart the pulse. Mouse and controller presses use the same animation.

Theme `keyboard_opacity` applies to Keyboard and TextInput; `ui_opacity` applies
to Settings, Mappings, and both pickers. Their defaults are `0.7` and `1.0`,
respectively; both accept finite values from `0.0` to `1.0`. They multiply
`background_color` alpha while the window is transparent. An opaque window
uses an opaque background. Move mode keeps its transparent window and themed
ghost outline.

Opacity belongs in the theme file. The former `config.toml` opacity keys are
ignored, and the opacity rows have been removed from Settings. Old Steam
Controller uses full opacity so its dark gaps retain the screenshot's contrast.

## Stick/pad cursors

```toml
[stick_pad_cursors]
appearance = "ring"
radius = 10.0
ring_thickness = 2.0
ring_fill_opacity = 0.2
opacity = 0.8
left_color = [70, 170, 255]
right_color = [255, 160, 70]
```

`solid` fills the disc; `fade` fades toward a transparent edge; `ring` fills
the center translucently with the outline color. `ring_fill_opacity` scales
the outline opacity for the fill; 0 leaves the center clear. Radius and
thickness use screen points. Dimensions must be finite and nonnegative;
opacity must be between 0 and 1. Ring
thickness is capped at the radius. Omit any field to use its config value.

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
colour. Space (`" "`) is valid. Later groups override earlier groups per
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
