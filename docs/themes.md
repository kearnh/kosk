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
[Portal 2](../themes/portal.toml), [Factorio](../themes/factorio.toml),
[Cyberpunk 2077](../themes/cyberpunk-2077.toml),
[Hollow Knight](../themes/hollow-knight.toml), and
[Stardew Valley](../themes/stardew-valley.toml) themes.
Add `themes/*.toml` to the `themes` list to discover them.

The Portal 2 theme uses clean gray-white surfaces with blue and orange accents.

The Factorio theme uses charcoal and steel panels with amber and copper accents.
Its background uses Factorio's original refined-concrete floor texture,
visible through dark translucent keyboard panels.
The asset's source and ownership are recorded in [themes/images/README.md](../themes/images/README.md).

The Cyberpunk 2077 theme uses black chrome, electric yellow, cyan and hot pink,
with square keys and neon ring cursors.

The Hollow Knight theme uses midnight blue, bone white and dream violet,
with rounded keys and soft soul-glow cursors.

The Stardew Valley theme uses parchment, walnut, leaf green and harvest gold,
with wooden borders and solid green/gold cursors.

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

## Background images

An optional `[background_image]` section draws a local image behind the controls.
PNG and JPEG are supported, along with self-contained SVG shapes. Relative paths
start beside the theme file; absolute paths are also accepted. Keep the image
alongside the theme when sharing it.

```toml
[background_image]
path = "images/factorio-refined-concrete.png"
opacity = 0.65
source_region = [0, 0, 512, 512]
scaling = "cover"
```

`opacity` defaults to `1.0` and accepts finite values from `0.0` to `1.0`.
In a transparent window, the image also inherits the background colour's alpha
and the current mode's `keyboard_opacity` or `ui_opacity`. Move mode hides images.
Transparent image pixels reveal `background_color`. Opaque controls cover the
image; translucent control backgrounds allow it to show through.

`scaling` defaults to `cover`, which preserves aspect ratio and crops to fill
the window. `contain` preserves aspect ratio and fits the whole image, leaving
the background colour in unused space. `stretch` fills the window without
preserving aspect ratio. `original` uses one image pixel per UI point, which
suits small corner decorations. `position` defaults to `center`; `top_left`,
`top_right`, `bottom_left` and `bottom_right` are also supported. Images are
clipped to the window and do not affect its layout or size.

`source_region = [x, y, width, height]` selects a rectangle in image pixels;
omitting it uses the whole image. A positive `frame_border` draws only that
many pixels around the source rectangle as a window frame. Corners keep their
size, edges stretch with the window, and the centre stays clear. Frame drawing
fills the window regardless of `scaling` or `position`. The default border is
`0`, which draws the image normally. The source region must fit inside the
image and leave a centre larger than zero after subtracting both borders.

Loading and decoding run on a worker thread. kosk retains only the active
background texture and runs at most one image-loading job at a time. Images
reload after image or theme edits. Missing, corrupt or oversized images retain
the theme's background colour and display a notice. Files are limited to
16 MiB and dimensions to 4096 pixels per side. SVGs rasterize once at their
declared size; embedded images and external image references are ignored.

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
Selected keys use `selection_text_color`; optional `left_selection_text_color`,
`right_selection_text_color`, and `dual_selection_text_color` override it for
each highlight. Selection text overrides key-group text; explicit layout text
retains priority. Layout fills keep their ordinary text colors.
Selection and key-group fills override the state background.

`open` has no current caller in kosk. Keyboard labels use explicit colours,
so `keyboard.hovered.text_color` and `keyboard.active.text_color` have no effect.
Their backgrounds and borders still apply. All state sections are optional;
keep only the overrides your theme needs.

## Fields

- Top level: `background_color`, `keyboard_opacity`, `ui_opacity`, `text_color`, `muted_text_color`,
  `selection_background_color`, `selection_border_color`,
  `selection_border_width`, `window_border_color`, `window_border_width`,
  `window_corner_radius`.
- `[background_image]`: `path`, `opacity`, `scaling`, `position`, `source_region`, `frame_border`.
- `[noninteractive]`, `[inactive]`, `[hovered]`, `[active]`, `[open]`: `background_color`,
  `weak_background_color`, `text_color`, `border_color`, `border_width`,
  `corner_radius`. These style standard controls, including both pickers.
  Top-level `text_color` supplies their text color unless a control state overrides it.
- `[keyboard.inactive]`, `[keyboard.hovered]`, `[keyboard.active]`: the same
  control fields, applied to keyboard keys.
- `[keyboard]`: `selection_background_color`, `selection_text_color`,
  `left_selection_color`, `right_selection_color`, `dual_selection_color`,
  `left_selection_text_color`, `right_selection_text_color`, `dual_selection_text_color`,
  `modifier_text_color`, `key_press_color`, `key_press_duration_ms`.
- `[[keyboard.key_groups]]`: `keys`, `background_color`, `text_color`.
- `[suggestions]`: `background_color`, `text_color`,
  `selected_background_color`, `selected_text_color`, `empty_slot_background`,
  `armed_color`, `disarmed_color`, `new_word_mark_color`, `corner_radius`,
  `selected_outline_width`.
- `[text_input]`: `background_color`, `text_color`, `cursor_color`.
- `[stick_pad_cursors]`: `radius`, `appearance` (`solid`, `fade`, `ring`),
  `opacity`, `ring_thickness`, `ring_fill_opacity`, `left_color`, `right_color`,
  `left_fill_color`, `right_fill_color`.
  Supplied values override config; omitted values use config defaults. Colors accept palette
  names or RGB/RGBA arrays. Cursor visibility stays in config.
- `[menus]`: `heading_color`, `muted_text_color`, `selected_text_color`.
  Selected Settings rows, including theme choices, use `selected_text_color`
  for labels and values against the top-level `selection_background_color`.
  Omit it to use the ordinary menu text color. Unselected rows use the
  noninteractive text color (inherited from top-level `text_color`).
- `[mappings]`: `row_focus_color`, `table_focus_color`, `focus_border_color`,
  `focus_border_width`, `text_color`, `warning_color`, `error_color`,
  `success_color`, `unsaved_color`, `editor_background_color`,
  `editor_border_color`, `editor_border_width`, `editor_corner_radius`.
- `[move_window]`: `background_color`, `border_color`, `border_width`,
  `corner_radius`, `text_color`, `text_shadow_color`.
- `[notifications]`: `background_color`, `border_color`, `border_width`,
  `corner_radius`, `muted_text_color`, `info_color`, `warning_color`, `error_color`.
- `[battery]`: `empty`, `low`, `medium`, `high`, `full`, `charging`, `unknown`.

Key presses briefly inset the key and flash its outline with `key_press_color`.
A contrasting outline keeps the effect visible when the flash matches the fill.
The fill and text colors stay unchanged during the animation.
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
left_fill_color = [20, 40, 60]
right_fill_color = [60, 40, 20]
```

`solid` fills the disc; `fade` fades toward a transparent edge; `ring` fills
the center translucently. Fill colors default to the corresponding outline
colors. `opacity` scales both outline and fill together. Fill alpha also
multiplies `ring_fill_opacity`; 0 leaves the center clear. Radius and
thickness use screen points. Dimensions must be finite and nonnegative;
opacity must be between 0 and 1. Ring
thickness is capped at the radius. Omit any field to use its config value.
Visible cursor outlines have a thin contrasting edge just outside the cursor
radius. The edge shares the cursor's opacity; transparent outlines have no edge.

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
