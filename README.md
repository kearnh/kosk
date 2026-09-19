# KOSK

KOSK is a Windows on-screen keyboard overlay driven by a game controller. It sits above other windows without taking focus, so you can type into whatever application is already active. Analog sticks (or Steam Controller pads mapped as sticks) highlight keys; triggers, face buttons, and chords send those keys, switch modes, or move the overlay.

The process is an [eframe](https://github.com/emilk/egui) application. Configuration lives in TOML next to the binary’s working files. DualShock 4 and Steam Controller 2 are the supported HID families; a recording format can replay a session without a physical pad.

## Running

From the repository root, pass a config file as the first argument:

```text
cargo run -- config.toml
```

Useful flags are documented in [docs/config.md](docs/config.md). `--replay FILE` plays a `.krec` tape instead of opening HID. `--keys-log FILE` (or `-` for stdout) writes outgoing keystrokes instead of injecting them.

## Completion next-word setup

Git ships prefix completion only (`data/completion/en/unigrams.tsv`). After a space, next-word chips need packed **pair** counts. `cargo build` does not pack them. Those files are gitignored.

This is **not** enough (`wrote N words, 0 bigrams`; chips stay `you` / `i` / `the`):

```text
cargo run --bin completion_build -- --unigrams data/completion/en/unigrams.tsv --out data/completion/en
```

1. Download [Norvig count_2w.txt](https://norvig.com/ngrams/count_2w.txt) (Google Web 1T top bigrams). Save as `data/completion/en/count_2w.txt`. Lines are `word1 word2<TAB>count`. Words missing from the unigram list are skipped.

2. Pack (this is the setup):

```text
cargo run --bin completion_build -- --unigrams data/completion/en/unigrams.tsv --bigrams data/completion/en/count_2w.txt --out data/completion/en
```

The last line must show a **non-zero** bigram count. `0 bigrams` means next-word stays top unigrams. Expect `vocab.txt`, `unigrams.bin`, `bigrams.bin` under `data/completion/en/`. `[completion.ngram] model_dir` must point there (relative to the config file).

Optional: `--corpus FILE` (one sentence per line; none is in this repo) or `--trigrams` with [count_3w.txt](https://norvig.com/ngrams/count_3w.txt).

3. Verify before launching the overlay:

```text
cargo run --bin completion_dev -- --text "going " --cursor 6 --backend ngram --model_dir data/completion/en
```

Pass: first chips are context words (`to`, …), not `you` / `i` / `the`. Fail: that trio, or stderr `ngram model at … not loaded; unigram-only` / `no bigrams`.

See [docs/completion.md](docs/completion.md).

## Reading the code

Start with [docs/README.md](docs/README.md). That page lists one explanation document per subsystem, written so each file can be read on its own.
