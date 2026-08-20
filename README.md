# KOSK

KOSK is a Windows on-screen keyboard overlay driven by a game controller. It sits above other windows without taking focus, so you can type into whatever application is already active. Analog sticks (or Steam Controller pads mapped as sticks) highlight keys; triggers, face buttons, and chords send those keys, switch modes, or move the overlay.

The process is an [eframe](https://github.com/emilk/egui) application. Configuration lives in TOML next to the binary’s working files. DualShock 4 and Steam Controller 2 are the supported HID families; a recording format can replay a session without a physical pad.

## Running

From the repository root, pass a config file as the first argument:

```text
cargo run -- config.toml
```

Useful flags are documented in [docs/config.md](docs/config.md). `--replay FILE` plays a `.krec` tape instead of opening HID. `--keys-log FILE` (or `-` for stdout) writes outgoing keystrokes instead of injecting them.

## Reading the code

Start with [docs/README.md](docs/README.md). That page lists one explanation document per subsystem, written so each file can be read on its own.
