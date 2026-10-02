import unittest

from build_english_bigrams import count_pairs


class PairCountingTests(unittest.TestCase):
    def test_export_metadata_is_not_counted(self):
        counts, sentences = count_pairs(["123\teng\tGoing home.\t2026-10-03"])
        self.assertEqual(counts, {("going", "home"): 1})
        self.assertEqual(sentences, 1)

    def test_sentence_boundaries_do_not_create_pairs(self):
        counts, _ = count_pairs([
            "1\teng\tGo home. Stay here! Come back? Thank you.\t2026-10-03",
            "2\teng\tGo home.\t2026-10-03",
        ])
        self.assertEqual(counts, {
            ("go", "home"): 2,
            ("stay", "here"): 1,
            ("come", "back"): 1,
            ("thank", "you"): 1,
        })

    def test_contractions_and_unicode_are_normalized(self):
        counts, _ = count_pairs(["1\teng\tI don’t mind cafe\u0301.\t2026-10-03"])
        self.assertEqual(counts, {
            ("i", "don't"): 1,
            ("don't", "mind"): 1,
            ("mind", "café"): 1,
        })

    def test_wrong_export_is_rejected(self):
        for row in ["1\teng\tMissing date", "1\tfra\tBonjour monde.\t2026-10-03"]:
            with self.subTest(row=row), self.assertRaises(ValueError):
                count_pairs([row])

    def test_empty_corpus_is_rejected(self):
        with self.assertRaises(ValueError):
            count_pairs([])


if __name__ == "__main__":
    unittest.main()
