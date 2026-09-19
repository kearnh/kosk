# Text completion — implemented vs not

Shipped: pluggable `CompletionBackend`, ngram + frequency dictionary, versioned worker, typed log, chips in Keyboard and TextInput, bumper cycle, accept/submit via mapping `when = "suggestionSelected"`, armed latch, distance-1 typo matching (layout neighbors, transpose, omit/extra key), full-word correction slot, user-cache next-word continuations. See [../completion.md](../completion.md).

Not in v1: network/LLM backends, OS caret scrape, CJK IME, neural rerank, next-char hitbox bias ([next-char-hitbox.md](next-char-hitbox.md)).
