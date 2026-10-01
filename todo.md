Todo
====

Controller / bindings
---------------------

- Warn (log or UI) when a mapping uses an input the detected controller does not
  support (e.g. `quickAccess`, paddles on DualShock 4). Today unsupported
  buttons simply never fire because device drivers report them as not held.

Input Text Prediction
---------------------

Left: neural rerank, OS caret scrape.
See [docs/plans/completion.md](docs/plans/completion.md).

Themes
------

- Add optional static background images: local PNG/JPEG paths relative to the
  theme file, opacity and scaling options, and background-colour fallback.
  Load asynchronously; cache only the active texture; limit image dimensions.
  Preserve window transparency and move mode. Handle image reloads and missing
  files. Check readability, resize cropping, memory use and repaint cost.

Bugs
----

- Debounce no 100% right. Easy to send triple key when intending to only send 2.
