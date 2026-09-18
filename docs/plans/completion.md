# Text completion — implemented vs not

Shipped: pluggable `CompletionBackend`, ngram + frequency dictionary, versioned worker, typed log, chips in Keyboard and TextInput, bumper cycle, `enterOrAcceptSuggestion`, armed latch. See [../completion.md](../completion.md).

Not in v1: typo/spatial model, network/LLM backends, OS caret scrape, CJK IME, neural rerank, next-char hitbox bias ([next-char-hitbox.md](next-char-hitbox.md)).
