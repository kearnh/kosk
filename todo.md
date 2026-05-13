Todo
====

Menu
----

Finish menu: nicer interface, config/options setting in menu, controller
navigation in move window

Input Text Prediction
---------------------

Goal: while the user is in `text_input` (single-line field above the on-screen
keyboard in `src/state/text_input.rs`), show **word completions** or short
**next-token suggestions** derived from the text before and after the cursor (completetion engine decides what to use, but both prefix suffix passed to ti), and let them
pick a suggestion without typing every character—especially useful with a
controller.

### Initial Steps

Done: engine skeleton, tests, and `completion_dev` — see [docs/completion.md](docs/completion.md).

- Find and evaluate crates that support what we need to do out of the box. Does any exist?
- Research text completion methods, any public papers / write ups?
- Start with a completion engine first, initially just in its own module. Add a new binary for it to test out independantly from the main osk.

### UX (sketch)

- Render a **strip or column of suggestion chips/buttons** above the text field or
  between the field and the keyboard (layout TBD).
- **Controller**: add an action to cycle focus across suggestions, action can be mapped to, focussing automatically inserts it at the cursor (replace current partial word or append after whitespace—define one rule and stick to it), additional action to cancel 
- **Mouse** click a chip to accept.

### Behavior

- Trigger completion refresh when `text` or `cursor_pos` changes (debounce
  slightly if using anything heavier than a prefix table).
- Empty prefix: optional **frequent words** or no suggestions.

### Implementation notes

- Abstract the completion engine so it can be changed out, or even have multiple sources
- Pass prefix and suffix to engine
- **Data source (pick one to start):** static word list / SCOWL-style list;
  optional user wordlist in config dir later; avoid network unless explicitly
  opted in (privacy, offline).
- **State:** keep suggestion state on `TextInputState` (or a small submodule):
  `Vec<String>`, `selected_index`, last `prefix` used to
  avoid redundant work.
