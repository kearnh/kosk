# Show and hide KOSK

KOSK starts hidden. Press **F3** to show or hide it. It stays running while hidden and accepts only visibility bindings. Both transitions clear word suggestions and their typing context. Hiding clears modifiers and releases held keys. Release held controller buttons before typing again.

The blue **K** in the Windows notification area toggles visibility when clicked. Right-click it for **Show KOSK** / **Hide KOSK** and **Quit KOSK**. Windows may place the icon in the notification area's overflow menu. The overlay has no taskbar button.

Showing leaves the current application focused. Mouse-pointer placement, including `--at-mouse`, follows the cursor on every show. Other positions are retained. Hiding a binding editor cancels its keyboard capture. Quit closes KOSK.

Turn **Start hidden** off in settings, or set `start_hidden = false` at the top level of your settings file. `--start-hidden` and `--start-visible` override the setting for that launch.

Map `toggleOverlayVisibility` in any mapping table to show or hide KOSK with a controller button. It also works while hidden. The mappings editor and binding picker use the keyboard's visibility bindings. Layout keys can use the same action. For example, in `mappings.toml`:

```toml
[Keyboard]
quickAccess = "toggleOverlayVisibility"
```

Bind each mode where you need it. Keep visibility bindings unconditional if the same button must always reopen KOSK.

To change the global shortcut, add this to the top level of your settings file:

```toml
show_hide_shortcut = "F3"
```

Use F1-F24, optionally with `Ctrl`, `Alt`, `Shift`, or `Win`, such as `"Ctrl+Alt+F3"`. Set `""` to disable it. Changes apply while KOSK runs. If registration fails, KOSK shows a notice and stays visible; the tray controls remain available.

## Steam desktop controls

Map a Steam button to send the same shortcut and switch to an action layer where only that button is bound. Set that button in the layer to send the shortcut and leave the layer. Reserve the button in KOSK's mappings so it cannot type or trigger another action.

Steam must switch its layer on both presses. Hiding KOSK does not restore Steam's desktop bindings.
