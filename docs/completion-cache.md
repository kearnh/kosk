# Completion cache tool

Build with `cargo build --bin completion_cache`. The executable is
`target/debug/completion_cache.exe`; use `--release` for `target/release`.

Quit KOSK before replacing its cache. The default cache is
`%LOCALAPPDATA%\kosk\completion-cache.bin`; a configured path may differ.

## Inspect

```powershell
$cache = "$env:LOCALAPPDATA\kosk\completion-cache.bin"
target/debug/completion_cache.exe export $cache
target/debug/completion_cache.exe export $cache --type '*' --kind unigram --prefix he
target/debug/completion_cache.exe export $cache --kind bigram --previous hello
target/debug/completion_cache.exe export $cache --min-count 2 --sort count --reverse --limit 20
target/debug/completion_cache.exe export $cache --since 1700000000 --compact | jq '.buckets[].unigrams[]'
```

JSON goes to stdout; diagnostics go to stderr. `--output FILE` writes JSON to a
new file. `--output -` also selects stdout. Existing JSON files are never overwritten.

Repeat `--type` to include multiple types. `*` is the catch-all type. `--word`,
`--prefix`, and `--contains` are case-sensitive and match either word of a bigram.
`--previous` matches only the previous word and excludes unigrams. Filters combine
with AND; different word filters may match different words of the same bigram.
Count and timestamp bounds are inclusive. Timestamps are Unix seconds.

`--sort word|count|time` sorts ascending within each type and kind; `--reverse`
reverses that order. `--limit` applies after sorting, separately to each type and
kind. Types sort alphabetically. Empty selected buckets remain in the output.

## Edit

Export the **full cache**, edit the JSON, validate, then import:

```powershell
target/debug/completion_cache.exe export $cache --output cache.json
# Edit cache.json in your editor.
target/debug/completion_cache.exe import cache.json --output $cache --dry-run
target/debug/completion_cache.exe import cache.json --output $cache --replace
```

Or pipe a JSON transformation through `jq`:

```powershell
target/debug/completion_cache.exe export $cache |
  jq '(.buckets[].unigrams[] | select(.word == "hello") | .count) = 10' |
  target/debug/completion_cache.exe import - --output $cache --replace
```

To remove a word and every pair containing it:

```powershell
target/debug/completion_cache.exe export $cache |
  jq '.buckets |= map(.unigrams |= map(select(.word != "hello")) | .bigrams |= map(select(.word != "hello" and .previous_word != "hello")))' |
  target/debug/completion_cache.exe import - --output $cache --replace
```

Import replaces the complete cache; it does not merge. Importing filtered exports
therefore deletes omitted entries. Use a separate `--output` path to create a subset.
Removing a unigram alone does not remove its bigrams.

Replacing an existing file requires `--replace` and first saves `<output>.bak`.
An existing backup blocks replacement; move it before the next edit. Writes use
`<output>.tmp`, then rename it. Existing temporary files also block writing.
Invalid JSON never changes the output. `--dry-run` only validates the document.

## JSON schema

```json
{
  "version": 2,
  "buckets": [
    {
      "app_type": "*",
      "unigrams": [
        { "word": "hello", "count": 2.0, "last_seen": 1700000000 }
      ],
      "bigrams": [
        { "previous_word": "hello", "word": "world", "count": 1.0, "last_seen": 1700000000 }
      ]
    }
  ]
}
```

All fields are required. Unknown fields, duplicate types/entries, empty words,
whitespace in words, negative/non-finite counts, unsupported versions, and malformed
binary input are rejected. Counts are 32-bit floats. The tool preserves word spelling
and timestamps. KOSK applies its configured word policy when loading and may prune
imported words; use normalized lowercase words that KOSK can learn.
