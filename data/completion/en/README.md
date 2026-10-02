# English completion data

`unigrams.tsv` derives from Hermit Dave's [FrequencyWords English 2018 list](https://github.com/hermitdave/FrequencyWords/blob/master/content/2018/en/en_50k.txt),
generated from the OpenSubtitles 2018 corpus. FrequencyWords licenses its
[content under CC BY-SA 4.0](https://github.com/hermitdave/FrequencyWords#license),
separately from its MIT-licensed code.

The KOSK list contains edits and reordered rows. A comparison on 2026-10-02
found 49,961 identical word/count rows among the 50,000 rows in each list.
The modified list is distributed under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
KOSK's MIT license does not replace these data terms.

`bigrams.tsv` contains pair counts derived from Tatoeba contributors'
[English CC0 sentence export](https://downloads.tatoeba.org/exports/per_language/eng/eng_sentences_CC0.tsv.bz2).
[Tatoeba's download documentation](https://tatoeba.org/en/downloads) identifies
this export as CC0; [its terms](https://tatoeba.org/en/terms_of_use) permit reuse.
The source and derived pair counts are available under
[CC0 1.0](../../../assets/licenses/CC0-1.0.txt).

The pinned source is `tatoeba-eng-cc0.tsv.bz2`, exported 2026-09-26 and downloaded
2026-10-03. It contains 41,512 sentences.
Source SHA-256: `88741ad33d7e71adca99de65b10f592584b6d85f073b8f8c454bc3ae9dab4c2a`.

`scripts/build_english_bigrams.py` reads only sentence text, normalizes Unicode
to NFC, lowercases words, converts curly apostrophes, and counts adjacent
tokens. Pairs do not cross sentence rows or `.`, `!`, and `?` boundaries.
Export IDs, language tags, and timestamps are excluded. Output is sorted
lexicographically and contains 256,637 distinct pairs.
Pair-count SHA-256: `918f76a076158c4c52af2d802c3051f37cded9ca2240c81277be57d754b10147`.

The model builder retains 218,581 pairs whose words occur in KOSK's vocabulary.
Generated vocabulary and unigram tables retain the FrequencyWords attribution
and CC BY-SA 4.0 terms above. Include this notice and both data licenses with
the prepared model. These data licenses do not replace KOSK's MIT code license.
