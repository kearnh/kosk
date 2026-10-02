# KOSK

[Download and start](#download-and-start) · [Controls](#controls) · [Steam setup](#steam-desktop-configuration) · [Customization](#advanced-customization)

An on-screen keyboard primarily designed for the Steam Controller. KOSK sits above other windows without taking focus and types into the application you are using.

KOSK is a personal project made for my own use and shared publicly under the MIT license. You're welcome to use and modify it. I'm sharing it without a commitment to provide support, respond to issues, or implement requests.

## Download and start

1. Download the Windows x64 ZIP from this repository's **Releases** section.
2. Extract the whole ZIP into a folder of your choice.
3. Connect your controller and open `kosk.exe`.
4. Press **Ctrl+Alt+F10** to show KOSK, or click the blue **K** in the Windows notification area. The icon may be in the overflow menu.

KOSK starts hidden. The same shortcut or tray icon hides it again while keeping it running. Right-click the tray icon to quit.

No installer, terminal, administrator rights, or first-launch download is required. Default layouts, mappings, themes, and English word suggestions, including next-word prediction, are included for offline use.

The alpha is **unsigned**. Windows may warn about an unrecognized application, and some security policies may prevent it from running. Use the ZIP from this repository's Releases section; do not disable Windows protection to run KOSK.

## Requirements and alpha limitations

- **Windows 11 x64.**
- Controller drivers target **Steam Controller 2** and **DualShock 4**. See release notes for verified controller revisions and connection methods.
- Linux support is intended for the future; there is no Linux release yet.
- The alpha has no automatic startup or updater.

<!-- Screenshot placeholder: replace with a keyboard screenshot before release. -->
*Screenshot coming soon.*

## Controls

These are the default bindings. Custom mappings can change them; the on-screen hints reflect your bindings. Steam Controller face buttons are A/B/X/Y; their DualShock 4 equivalents are Cross/Circle/Square/Triangle.

- **Pads or sticks:** move the left and right key selections.
- **Left/right trigger:** enter the selected key on that side, or accept a highlighted word suggestion.
- **Pad click:** enter the key selected by that pad.
- **A / Cross:** Enter, or accept a highlighted suggestion.
- **X / Square:** Backspace. **R5:** Space.
- **Y / Triangle** or **L4:** toggle Shift. **Left/right stick click:** toggle Ctrl/Alt.
- **D-pad:** arrow keys.
- **Left/right bumper:** select the previous/next word suggestion. **B / Circle:** clear the suggestion selection.
- **Hold the left button beside Steam, then press Y:** open settings. On DualShock 4, hold Share and press Triangle.
- **Hold that same left button, then press B:** open text-input mode. On DualShock 4, hold Share and press Circle.
- **Quick Access (⋯):** open the mappings editor.

The keyboard also has selectable keys for settings, modifiers, and switching between letters and symbols. Choose other layouts through **Settings → Layouts**.

In settings, use up/down to select a row, left/right to change a value, A/Cross to confirm, and B/Circle to go back. **Settings → Move window** lets you reposition the overlay; A/Cross saves, B/Circle leaves without saving.

Text-input mode holds a line inside KOSK until you submit it. Submission sends the line and Enter to the focused application, then returns to the keyboard. Normal keyboard mode sends each key immediately.

**Ctrl+Alt+F10** shows or hides KOSK without changing application focus. Hiding releases held keys and clears modifiers; both showing and hiding clear suggestion context. Release held controller buttons before resuming. See [visibility controls](docs/show-hide.md) for shortcut options.

## Word suggestions

KOSK offers word completions, corrections, and next-word suggestions. The packaged English data needs no setup.

Use the bumpers to highlight a suggestion, then press a trigger or A/Cross to accept it. B/Circle clears the highlight so those buttons resume their usual actions.

Immediately after accepting a suggestion, **L5** deletes one character and offers your original text as the first suggestion. Selecting it restores that text without adding a space. Further edits or cursor movement dismiss the offer; ordinary Backspace still deletes one character.

Hold the left button beside Steam and press X to toggle suggestions; on DualShock 4, hold Share and press Square. **Settings → Options → Suggestions** controls their behavior and appearance, including whether accepted words and submitted lines are learned. Learned words are stored locally under `%LOCALAPPDATA%\kosk`.

In normal keyboard mode, suggestions follow text entered through KOSK. They do not read the destination application's text. Cursor movement and pasting can pause suggestions because KOSK no longer knows the surrounding text.

## Settings and appearance

Open settings to change controller feel, key size, text size, repeat behavior, suggestions, and overlay placement. Select **Themes** to apply an appearance, **Layouts** to choose a keyboard, or **Mappings** to change button actions.

Bundled themes are available automatically: Old Steam Controller, Portal 2, Factorio, Cyberpunk 2077, Hollow Knight, and Stardew Valley.

Personal settings live in `%LOCALAPPDATA%\kosk\config.toml`. KOSK creates this file and stores your changes there; it need not contain every default setting. In settings, the left button beside Steam (Share on DualShock 4) opens the file in your editor.

## Steam desktop configuration

You can keep Steam's normal desktop controls and switch them off while using KOSK. Steam must change its action layer at the same time that it sends KOSK's visibility shortcut.

1. Set KOSK's shortcut to F3 by adding this top-level setting to `config.toml`, before any `[section]` headings:

   ```toml
   show_hide_shortcut = "F3"
   ```

2. In Steam's desktop configuration, bind **R4** to send F3 and enter a dedicated KOSK action layer.
3. In that layer, remove all inherited desktop actions. Bind only R4: send F3 and leave the layer.
4. Leave R4 unbound in KOSK. It is unbound in the default mappings.

Start KOSK once. R4 then shows KOSK and enters the empty Steam layer; pressing it again hides KOSK and restores Steam's desktop controls. No AutoHotkey script is needed.

Do not also bind R4 to KOSK's own visibility action: the same press would toggle twice. Using the tray or another shortcut changes only KOSK's visibility, so it can leave Steam's layer out of sync. Hiding KOSK does not restore Steam's desktop bindings by itself.

## Advanced customization

Custom files use TOML. Relative paths start beside `config.toml`, except theme image paths, which start beside the theme file. Keep custom files separate from bundled defaults so updates preserve your edits. Top-level options such as `themes` and `controller_map` belong before any `[section]` headings.

### Create a layout

Copy [the main layout](old_sc.toml) or [the symbols layout](old_sc_symbols.toml) into a custom file, such as `%LOCALAPPDATA%\kosk\layouts\mine.toml`. Register it in `config.toml`:

```toml
[layouts]
mine = "layouts/mine.toml"
```

Choose it through **Settings → Layouts**. Keep a `main` layout registered.

Layouts define rows, key widths, labels, left/right selection bounds, and resting positions. A key can send a character, a string, a named key such as Backspace, or an action such as switching layouts. For example, within a row:

```toml
[[rows.items]]
key = { normal = "1", shift = "!" }
width = 1.2

[[rows.items]]
key = "switchLayout.main"
display = "Letters"
```

`display` changes the label independently of the action. Conditional display rules can change labels or colours when Shift is active or a suggestion is selected. See the [layout format](docs/developer/keyboard-layout.md) for fields and examples.

### Create a theme

Save a theme as `%LOCALAPPDATA%\kosk\themes\amber.toml`, for example:

```toml
name = "Amber"
background_color = [24, 20, 16, 255]
keyboard_opacity = 0.8
ui_opacity = 1.0

[keyboard]
left_selection_color = [175, 100, 30, 255]
right_selection_color = [100, 130, 45, 255]
dual_selection_color = [150, 85, 110, 255]
```

Add your custom theme files to the top-level `themes` list in `config.toml`:

```toml
themes = ["themes/*.toml"]
```

Select the theme through **Settings → Themes**. Omitted fields inherit the built-in appearance. Colours accept RGB, RGBA, or names from a `[colours]` palette. Themes can customize keys, menus, suggestions, cursors, borders, and background images. See the [theme guide](docs/themes.md).

### Add bindings and conditions

Use **Settings → Mappings**, or create `mappings.toml` beside `config.toml` and set this top-level option:

```toml
controller_map = "mappings.toml"
```

Your file only needs bindings that differ from the defaults. A binding belongs to a screen, such as `[Keyboard]` or `[TextInput]`. Use `"none"` to remove an inherited binding.

This example accepts a highlighted suggestion with the right trigger, but otherwise enters the key under the right selection:

```toml
[Keyboard]
triggerRight = [
    { action = "acceptSuggestion", when = "suggestionSelected" },
    { action = "sendKeyUnderRightStick" },
]
r4 = "none"
```

Rules are tried in order; the first matching rule wins. Put an unconditional fallback last. Without a matching rule or fallback, the button does nothing.

Conditions can combine flags with `&&`, `||`, `!`, and parentheses, such as `"completionActive && !modifier.ctrl"`. Two button names joined by `+` form a chord: hold the first, then press the second. The first button cannot also have its own binding on that screen.

See the [binding reference](MAPPINGS.md) for button names, actions, available screens, and all `when` conditions.

## Updates and removal

To update, quit KOSK, extract the new release into a new folder, and run its `kosk.exe`. Personal settings, custom themes, layouts, bindings, and learned words under `%LOCALAPPDATA%\kosk` are preserved.

To remove KOSK, quit it and delete the application folder. Delete `%LOCALAPPDATA%\kosk` as well only if you want to remove your settings, customizations, and learned words.

## AI development disclaimer

KOSK was coded using AI. I've made every effort to understand the code rather than treat it as a black box, including maintaining [developer documentation](docs/developer/README.md) explaining its architecture. This does not guarantee that the software is free of bugs.

## License and credits

KOSK code is [MIT licensed](LICENSE). Bundled [English completion data](data/completion/en/README.md) and [controller glyphs](assets/controller-glyphs/README.md) have separate terms and credits.
