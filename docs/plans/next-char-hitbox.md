# Next-character hitbox bias (plan)

Stick hit-testing today uses one precomputed hitbox per selectable key (`KeyboardLayout::calculate_hitboxes` → `get_key_at`). Overlaps resolve to the containing shape with the smallest distance. Highlighting and `sendKeyUnder*Stick` both go through that path (`handle_controller_input` writes `selected`; send reads it). Mouse clicks do not.

This plan: predict the next character from what kosk has already typed (keyboard mode is enough; text-input mode is optional extra context), and for that next press only, hit-test predicted keys with a **precomputed larger hitbox** and **prefer** them when they contain the cursor.

Word completion (`src/completion/`, [../completion.md](../completion.md), [completion.md](completion.md)) is a different feature (suggestion chips). It does not produce `P(next char)`. Do not overload `CompletionEngine::suggest` for this.

## Why

Analog aim is imprecise. After `"in th"`, `"e"` is the letter the user almost always wants. Growing that one hitbox for one press reduces neighbor hits (`w`/`r`/`d`) without permanently inflating every key.

## Prerequisite: next-char predictor

New module next to completion, e.g. `src/completion/next_char.rs` (or `src/prediction/`), UI-agnostic, sync, offline.

```text
trait NextCharPredictor {
    fn predict(&self, prefix: &str) -> NextCharDist;
}

struct NextCharDist {
    /// Folded lowercase (and space). Missing keys → 0.
    /// Values are probabilities, sum ≤ 1 (mass may sit on chars not on the layout).
    p: HashMap<char, f32>,
}
```

**v0 engine (enough for `"in th"` → `e`):** dictionary next-letter.

1. `token = word_prefix_token(prefix)` (reuse the completion helper).
2. If `token` is non-empty: scan dictionary words with that prefix (same `partition_point` as `DictionaryEngine`). Histogram the **next character after the token**. If the word *is* the token, count a word-boundary mass (space / punctuation — config).
3. Convert counts to probabilities.
4. If `token` is empty (start, or after space): histogram the **first letter** of the word list (or a unigram table if one is shipped). Char-level n-grams can replace this later; not required for v0.

Optional later: frequency-weighted counts, char n-gram backoff, user cache. Keep a trait so those swap in without touching hit-test.

**Dev binary:** print the distribution for a prefix (extend `completion_dev` or add `next_char_dev`). Needed for threshold tuning.

**Tests:** `"th"` on a list containing `the`/`this`/`that` puts most mass on `e`/`i`/`a`; empty token returns something defined; unknown prefix returns empty (no expansion).

## Typed context (no text-input required)

Kosk injects into other apps. There is no caret in the target. Context is **outgoing text kosk itself sent**.

Keep a short buffer on `KeyboardState` (not only on `TextInputState`):

- Append on accepted `RawKey::Key` / `RawKey::Text` once `push` / `push_seq` succeeds.
- Backspace: pop one char if `track_backspace`.
- Enter / Return: clear if `clear_on_enter`.
- Ctrl/Alt (and sticky-mod chords): do not append if `ignore_when_ctrl_or_alt`.
- Layout switch: clear if `clear_on_layout_switch`.
- Idle: if `idle_reset_ms > 0` and nothing was appended for that long, clear.
- Cap at `max_chars` (drop from the front).
- Fold case for the predictor; remember the buffer as typed so Shift after `"th"` still predicts `e` for the same physical key.

Text-input mode: when that mode is active, prefer its buffer + caret as `prefix` (it is ground truth). When it is not, use the keyboard rolling buffer. Same predictor, two context sources.

Mouse-sent keys go through `send_key` too; they update the buffer. They still do **not** use expanded hitboxes (no stick hit-test).

Prediction must not sit on the input path in a way that can drop a key. Lookup is a histogram over a prefix range; do it when the buffer changes, cache `NextCharDist` on the keyboard, hit-test only reads the cache.

## Precomputed hitboxes

`calculate_hitboxes` already runs after centres are captured. Compute **two** parallel grids in that same pass (and on config reload that changes scale):

| Grid | When used |
|------|-----------|
| `key_hit_boxes` | default (today’s circle / ellipse) |
| `key_hit_boxes_expanded` | that cell’s character is currently predicted above threshold |

Same centre. Expanded = multiply circle `r` and ellipse `rx`/`ry` by that character’s `scale` (default `scale`, per-letter override). Clamp with `min_scale` / `max_scale` so a typo in config cannot cover the board.

Do **not** recompute per poll. Only centres + config change rebuild geometry. Which grid is *consulted* changes per poll from the cached distribution.

`export_geometry` / debug draw: default overlay stays the normal grid. When `debug.show_predicted_hitboxes`, stroke the **active** expanded boxes in a second colour so tuning is visible. MCP snapshot can keep exporting the normal box unless a later change needs both.

## Hit-test change

`get_key_at` (and therefore `get_nearest_key_*`) takes the current prediction (or a pre-resolved set of `(row, col)` / chars).

For each selectable cell:

1. Resolve the cell’s character in the **unshifted** layer (`RawKey::Key` only; skip `Text` / `Enigo` / `Action` / `Skip`). Compare folded.
2. If that char is in the active expanded set, test `key_hit_boxes_expanded`; else test `key_hit_boxes`.
3. Collect hits.

**Active expanded set:** chars with `p(c) >= threshold(c)`, ranked by `p`, keep at most `max_expanded`. A char with no per-letter threshold uses `default_threshold`. Empty distribution → empty set → today’s behavior.

**Prefer (required, not implied by size):** a larger box still loses today’s min-distance rule if a neighbor centre is closer. Add an explicit rule, configurable:

- `prefer = "predicted"` (v0 default): if any expanded predicted box contains the point, ignore non-predicted hits; among predicted hits, keep min distance.
- `prefer = "distance"`: size only; today’s distance rule.
- `prefer = "blend"`: score = distance_norm * (1 − `blend_weight` * p); lowest wins. Extra knob for later.

`max_expanded = 1` plus `prefer = "predicted"` is the `"in th"` → `"e"` behavior.

Stick-select lock is unchanged: after a send, that stick stays on the sent key for `stick_select_lock_ms`. The *next* prediction applies after the lock drops (and after the buffer includes the char just sent).

Near rest: if stick magnitude < `rest_deadzone`, do not apply expansion (home-row `d`/`k` must not jump to a predicted neighbor at idle). `0` disables.

Shift: hitboxes are on cells, not glyphs. Match predicted `e` to the cell whose normal is `e` even when `shift_state` is on. `apply_when_shift = false` skips expansion while the shift layer is up.

Symbol layout: if the predicted char is not on the current layout, that expansion is a no-op. `apply_off_main_layout = false` disables the feature on non-`main` layouts.

## Config

Everything below is live-reloadable. Defaults must preserve today’s hit-test when the table is omitted or `enabled = false`.

Ship a commented example in `config.toml`. Per-letter maps are sparse: omitted letters use the default.

```toml
[next_char]
# "dictionary" = v0 (word-list next letter). Later: "ngram".
engine = "dictionary"
# Optional UTF-8 word list; omit → embedded demo list (same as completion).
# wordlist = "words.txt"

[predict_hitbox]
enabled = false
default_threshold = 0.35          # p(c) must be ≥ this to expand c
default_scale = 1.35              # multiplier on r / rx / ry
min_scale = 1.0
max_scale = 2.0
max_expanded = 1                  # top-k among those that pass threshold
prefer = "predicted"              # predicted | distance | blend
blend_weight = 0.5                # only for prefer = "blend"
min_probability_gap = 0.0         # require p(best) - p(second) ≥ this, else expand none
rest_deadzone = 0.12              # stick magnitude; 0 = always allow
apply_when_shift = true
apply_off_main_layout = false
ignore_when_ctrl_or_alt = true

[predict_hitbox.threshold]
# e = 0.25
# q = 0.60
# " " = 0.50

[predict_hitbox.scale]
# e = 1.5
# space uses the key whose RawKey is ' ' if the layout has one

[predict_hitbox.context]
max_chars = 32
idle_reset_ms = 2500              # 0 = never
clear_on_enter = true
clear_on_layout_switch = true
track_backspace = true
```

```toml
[debug]
show_predicted_hitboxes = false
```

**Other knobs worth having (easy to skip later, painful to add after wiring):**

| Knob | Why |
|------|-----|
| `min_probability_gap` | Two neighbors both at ~30% (`i` vs `e` after `"th"` on a bad list) should not both swell into each other. |
| `rest_deadzone` | Expansion at stick rest steals the home-row highlight. |
| `max_expanded` | Top-1 is the product; top-3 is a tuning experiment. |
| Per-letter `scale` | `"e"` can grow more than `"q"` at the same p. |
| `idle_reset_ms` | Buffer from the previous sentence in another app would poison the next word. |
| `ignore_when_ctrl_or_alt` | Chords are not letters; do not bias `c` after Ctrl. |
| `apply_when_shift` / `apply_off_main_layout` | Layer-specific feel. |
| `prefer` | Size-only vs hard prefer is the main feel axis; keep it a switch. |
| `enabled` | A/B against recordings with one flag. |

**Do not skip in tape overlay** (`TAPE_CONFIG_SKIP`). Replay must use the same thresholds as the recording, or hit-test diverges.

Relative `wordlist` path: resolve like other config paths (directory of the main TOML).

## Flow

```text
send_key accepted
  → update typed buffer / or TextInputState buffer
  → NextCharPredictor::predict(prefix)
  → cache dist + active expanded charset (threshold, k, gap)

controller poll
  → if stick mag < rest_deadzone: get_key_at(normal only)
  → else get_key_at(mixed grids + prefer rule)
  → selected / highlight / send unchanged from there
```

Geometry rebuild does not call the predictor.

## Tests (write first where this is a behavior change)

- `calculate_hitboxes` fills both grids; expanded radii = normal * scale (circle and ellipse).
- Below threshold: `get_key_at` identical to today (same point, same key).
- Point inside expanded `e` only (outside normal `e`, outside neighbor): returns `e` iff `e` is active.
- Point inside normal `w` and expanded `e`: `prefer = "predicted"` → `e`; `prefer = "distance"` → whatever today’s metric says.
- `max_expanded = 1` with `e` and `i` both above threshold: only the higher-p char uses the big box.
- `rest_deadzone`: near-zero stick does not use expansion.
- Buffer: append, backspace, idle clear, ctrl ignored.
- Predictor: `"th"` → `e` dominates on a fixture list.

Do not retune production defaults in those tests; pass scale/threshold in.

## Out of scope

- Suggestion chips / accept-suggestion actions ([completion.md](completion.md)).
- Reading the focused app’s text (no OS accessibility hook).
- Neural models.
- Changing mouse hit-testing.
- Interpolating a continuum of radii per poll (one expanded grid is the point of precompute). A later `prefer = "blend"` already covers soft ranking.

## Implementation order

1. `NextCharPredictor` + tests + tiny CLI.
2. Config structs + defaults-off in checked-in `config.toml` (commented table is enough).
3. Dual hitboxes + `get_key_at` prefer rule + layout unit tests (inject a fake active set; no predictor yet).
4. Typed buffer on `KeyboardState`; wire predictor cache; pass active set into hit-test.
5. Text-input buffer as context when that mode is current.
6. Debug overlay; enable in local `config.toml` and tune `default_threshold` / `default_scale` / `rest_deadzone` against real stick use.

Tuning is the product. Every constant above is config; do not bury new ones in `layout.rs`.
