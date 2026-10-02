# Show and hide KOSK

Press **F3** to show or hide KOSK. It stays running while hidden and ignores controller input. Both transitions clear word suggestions and their typing context. Hiding clears modifiers and releases held keys. Release held controller buttons before typing again.

The blue **K** in the Windows notification area toggles visibility when clicked. Right-click it for **Show KOSK** / **Hide KOSK** and **Quit KOSK**. Windows may place the icon in the notification area's overflow menu. The overlay has no taskbar button.

Showing preserves position and leaves the current application focused. Hiding a binding editor cancels its keyboard capture. Quit closes KOSK.

To change the global shortcut, add this to the top level of your settings file:

```toml
show_hide_shortcut = "F3"
```

Use F1-F24, optionally with `Ctrl`, `Alt`, `Shift`, or `Win`, such as `"Ctrl+Alt+F3"`. Set `""` to disable it. Changes apply while KOSK runs. If registration fails, KOSK shows a notice and stays visible; the tray controls remain available.

## Steam desktop controls

Map a Steam button to send the same shortcut and switch to an action layer where only that button is bound. Set that button in the layer to send the shortcut and leave the layer. Reserve the button in KOSK's mappings so it cannot type or trigger another action.

Steam must switch its layer on both presses. Hiding KOSK does not restore Steam's desktop bindings.
