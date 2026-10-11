# Factorio palette

Visual references from Wube:

- [GUI styles](https://www.factorio.com/blog/post/fff-348)
- [Green confirmation button](https://cdn.factorio.com/assets/img/blog/fff-362-logistic-request-window.png)
- [Factorio 2.0 settings](https://cdn.factorio.com/assets/blog-sync/fff-408-wayland.png)

Flat fills sampled from Factorio 2.1.21's `data/core/graphics/gui-new.png`,
using sprite coordinates in `data/core/prototypes/style.lua`:

- Frame `(8, 8)`: RGB `45, 44, 45`; shallow panel `(76, 8)`: `62, 61, 62`.
- Slot `(40, 776)`: `45, 45, 45`; selected `(120, 776)`: `255, 175, 0`;
  pressed `(200, 776)`: `220, 123, 0`; border `(4, 776)`: `36, 36, 36`.
- Button `(8, 25)`: `143, 143, 143`; hovered `(42, 25)`: `228, 153, 34`;
  pressed `(59, 25)`: `241, 191, 100`.
- Confirm `(76, 25)`: `94, 183, 99`; cancel `(144, 25)`: `254, 90, 90`.
- Text field `(256, 8)`: `131, 131, 131`; frame highlight `(8, 1)`: `93, 90, 88`.

Text and status colours follow `style.lua`: white body text, black button text,
cream captions, orange warnings, pink errors, blue information, and green glow.
Keys use opaque slot fills to preserve their colours over the game.
The theme approximates the GUI's flat colours; it does not copy its bevels or textures.
