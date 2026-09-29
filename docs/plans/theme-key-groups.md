# Theme key groups

Proposed schema; not implemented.

```toml
[[keyboard.key_groups]]
keys = ["Return", "exit"]
background_color = [63, 100, 58, 255]

[[keyboard.key_groups]]
keys = ["q", "w", "e", "r", "t"]
background_color = [125, 60, 65, 255]
text_color = [235, 220, 220, 255]
```

Each group needs a nonempty `keys` list and at least one color. Both colors
are optional four-byte RGBA arrays. Repeat the table for additional groups;
no group names, nested styles, or selector language are required.

## Matching and precedence

- Use the layout's key syntax: characters, Enigo key names such as `Return`,
  or keyboard actions such as `exit` and `toggleShift`.
- Match the normal key identity, keeping its group when Shift changes its
  output. Display labels and icons do not affect matching.
- Unmatched keys inherit the ordinary keyboard theme. Keys absent from a
  layout have no effect, allowing one theme to serve several layouts.
- Later groups override earlier groups per color. An omitted color leaves
  the earlier value intact.
- Layout display-rule colors retain priority. Controller selection fills
  override group backgrounds, keeping the selected keys distinguishable.
- Reject empty key lists, empty key strings, groups without colors, unknown
  properties, and malformed colors when loading a theme.

## Implementation

Add `key_groups` to the keyboard theme model. Keep serialized key strings in
that model; compile them with the existing key parser inside the keyboard
layer. Cache the compiled rules and refresh them when the theme changes.
Use the existing normal-key accessor; no symbol visibility changes are needed.

Resolve group colors before applying layout colors and selection fills.
Add the green `Return`/`exit` group to Old Steam Controller when support lands.

Test multiple groups, overlapping groups, normal/shifted keys, action keys,
label independence, fallback colors, layout priority, selection visibility,
invalid groups, and live theme changes.
