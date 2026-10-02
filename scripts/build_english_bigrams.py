"""Rebuild CC0 English pair counts from the pinned Tatoeba export."""

import argparse
import bz2
from collections import Counter
import hashlib
from pathlib import Path
import re
import unicodedata


REPOSITORY = Path(__file__).resolve().parent.parent
DATA_DIRECTORY = REPOSITORY / "data" / "completion" / "en"
SOURCE_SHA256 = "88741ad33d7e71adca99de65b10f592584b6d85f073b8f8c454bc3ae9dab4c2a"
TOKEN_PATTERN = re.compile(r"[\w']+")
SENTENCE_BOUNDARY = re.compile(r"[.!?]")
EXPORT_FIELD_COUNT = 4


def count_pairs(lines):
    counts = Counter()
    sentences = 0
    for number, line in enumerate(lines, 1):
        fields = line.rstrip("\r\n").split("\t")
        if len(fields) != EXPORT_FIELD_COUNT or not fields[0].isdigit():
            raise ValueError(f"Invalid Tatoeba export row {number}")
        if fields[1] != "eng":
            raise ValueError(f"Non-English Tatoeba export row {number}")

        text = unicodedata.normalize("NFC", fields[2]).replace("’", "'").lower()
        for sentence in SENTENCE_BOUNDARY.split(text):
            tokens = TOKEN_PATTERN.findall(sentence)
            counts.update(zip(tokens, tokens[1:]))
        sentences += 1

    if not counts:
        raise ValueError("Tatoeba export contains no word pairs")
    return counts, sentences


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source", type=Path, default=DATA_DIRECTORY / "tatoeba-eng-cc0.tsv.bz2"
    )
    parser.add_argument("--out", type=Path, default=DATA_DIRECTORY / "bigrams.tsv")
    args = parser.parse_args()
    source = args.source.read_bytes()
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise ValueError("Source differs from the pinned English CC0 export")

    lines = bz2.decompress(source).decode("utf-8").splitlines()
    counts, sentences = count_pairs(lines)
    with args.out.open("w", encoding="utf-8", newline="\n") as output:
        for (first, second), count in sorted(counts.items()):
            output.write(f"{first}\t{second}\t{count}\n")
    print(f"Counted {len(counts)} word pairs from {sentences} sentences")
    print(f"SHA-256: {hashlib.sha256(args.out.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
