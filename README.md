# KOSK

KOSK is a Windows on-screen keyboard overlay driven by a game controller. It sits above other windows without taking focus, so you can type into whatever application is already active. Analog sticks (or Steam Controller pads mapped as sticks) highlight keys; triggers, face buttons, and chords send those keys, switch modes, or move the overlay.

The process is an [eframe](https://github.com/emilk/egui) application. Configuration lives in TOML next to the binary’s working files. DualShock 4 and Steam Controller 2 are the supported HID families; a recording format can replay a session without a physical pad.

## Running

From the repository root, pass a config file as the first argument:

```text
cargo run -- config.toml
```

Useful flags are documented in [docs/config.md](docs/config.md). `--replay FILE` plays a `.krec` tape instead of opening HID. `--keys-log FILE` (or `-` for stdout) writes outgoing keystrokes instead of injecting them.

## Controller mappings

What each button does is in `mappings.toml`. Button names, action names, and `when` conditions are listed in [MAPPINGS.md](MAPPINGS.md).

## Word suggestions

While you type, kosk can show a few guesses above the keyboard.

To finish a word you started, you only need the English list already in the repo: `data/completion/en/unigrams.tsv`. Type `hel` and you should see words like `hello`.

Guessing the *next* word after a space is a separate step. That uses a table of common two-word sequences: how often `going` is followed by `to`, and so on. The table is large, so it is not in git. You build it once on your machine.

### Next word after a space

1. Download Peter Norvig's [count_2w.txt](https://norvig.com/ngrams/count_2w.txt). Save it as `data/completion/en/count_2w.txt`. Each line is two words and how often they appear together. Words that are not in the English list above are ignored.

2. Convert that file into the tables kosk loads:

```text
cargo run --bin completion_build -- --unigrams data/completion/en/unigrams.tsv --bigrams data/completion/en/count_2w.txt --out data/completion/en
```

`--unigrams` is the word list from the repo. `--bigrams` is the download from step 1. `--out` is the folder for the generated files (`vocab.txt`, `unigrams.bin`, `bigrams.bin`). Those generated files stay out of git. The word list is only read; it is not rewritten.

The last printed line includes how many two-word pairs were kept. If that number is zero, `--bigrams` was probably omitted, and after a space you will only see the most common English words (`you`, `i`, `the`).

`config.toml` already has `[completion.ngram] model_dir = "data/completion/en"`. The path is relative to the config file and should match `--out`.

If you have your own text, `--corpus FILE` counts pairs from one sentence per line. There is no corpus in this repo. For three-word sequences, add `--trigrams` and [count_3w.txt](https://norvig.com/ngrams/count_3w.txt).

3. Check without opening the overlay. This pretends you typed `going` and a space:

```text
cargo run --bin completion_dev -- --text "going " --cursor 6 --backend ngram --model_dir data/completion/en
```

You should see words that follow `going` (for example `to`). If the guesses are still `you` / `i` / `the`, the two-word table did not load. The same problem shows up on stderr as `ngram model at … not loaded; unigram-only` or `no bigrams`.

See [docs/completion.md](docs/completion.md) (app types, current-word chip, `cargo run -p wordlist-convert` recipes).

## Reading the code

Start with [docs/README.md](docs/README.md). That page lists one explanation document per subsystem, written so each file can be read on its own.
