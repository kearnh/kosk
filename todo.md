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

Bugs
----

- Debounce no 100% right. Easy to send triple key when intending to only send 2.
