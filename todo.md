Todo
====

Controller / bindings
---------------------

- Warn (log or UI) when a mapping uses an input the detected controller does not
  support (e.g. `quickAccess`, paddles on DualShock 4). Today unsupported
  buttons simply never fire because device drivers report them as not held.

Settings
--------

Finish settings: nicer interface, config/options setting on that screen

Input Text Prediction
---------------------

Left: neural rerank, OS caret scrape.
See [docs/plans/completion.md](docs/plans/completion.md).

Bugs
----

- Debounce no 100% right. Easy to send triple key when intending to only send 2.
- The config watcher does not watch `mappings.toml`. An edit in an external
  editor is ignored until something else reloads config (or the process
  restarts). An in-app mappings save notifies listeners itself.
