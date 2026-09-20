Todo
====

Controller / bindings
---------------------

- Warn (log or UI) when a mapping uses an input the detected controller does not
  support (e.g. `quickAccess`, paddles on DualShock 4). Today unsupported
  buttons simply never fire because device drivers report them as not held.

Menu
----

Finish menu: nicer interface, config/options setting in menu

Input Text Prediction
---------------------

Shipped: ngram/dictionary backends, chips, bumper cycle, enter-or-accept,
typo tolerance, next-word from packed bigrams or the user cache.
See [docs/completion.md](docs/completion.md). Left: neural rerank,
OS caret scrape.

Mappings UI
-----------

- Controller: Cancel/Save/Delete only via D-pad Down past every binding row.
  Add a jump-to-footer shortcut (e.g. Options) so the footer is reachable
  without scrolling the whole list.

Bugs
----

- Debounce no 100% right. Easy to send triple key when intending to only send 2.
- The config watcher does not watch `mappings.toml`. Edits to that file are
  ignored until something else reloads config (or the process restarts).
- `config::save` dumps the whole `config.toml` via `toml::to_string_pretty`
  whenever window position is persisted. `controller_map = "mappings.toml"` is
  loaded into a HashMap and the path is forgotten, so save rewrites inline
  `[controller_map.*]` tables and later mapping edits (e.g. pad click) never
  apply. Save should keep the file reference (or only patch `window_pos`).