import json
from pathlib import Path
import tempfile
import unittest

from compare_human_runs import check_completions, summarize


class ComparisonTests(unittest.TestCase):
    def test_transport_failure_is_not_a_model_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "responses.jsonl").write_text("")
            failures = directory / "failures.jsonl"
            failures.write_text(json.dumps({"task_id": "a", "error": "connection refused"}) + "\n")
            with self.assertRaisesRegex(ValueError, "transport failure"):
                check_completions(directory, {"tools": "none"}, {"a"})
            failures.write_text(json.dumps({"task_id": "a", "error": "no answer",
                                           "finish_reason": "stop"}) + "\n")
            check_completions(directory, {"tools": "none"}, {"a"})
            with self.assertRaisesRegex(ValueError, "Incomplete"):
                check_completions(directory, {"tools": "none"}, {"a", "b"})

    def test_missing_answer_stays_in_denominator(self):
        fixtures = [{"id": "a", "task": "write"}, {"id": "b", "task": "write"}]
        result = summarize(fixtures, {"a": {"score": 1.0}})["write"]
        self.assertEqual(result["total"], 2)
        self.assertEqual(result["correct"], 1)
        self.assertEqual(result["mean_score"], 0.5)
        self.assertEqual(result["failures"]["missing answer"], 1)

    def test_equivalence_is_separate_from_tree_weight(self):
        fixtures = [{"id": "a", "task": "tree"}, {"id": "b", "task": "tree"}]
        results = {
            "a": {"score": 0.0, "failure": "unimproved"},
            "b": {"score": 0.0, "failure": "wrong semantics"},
        }
        result = summarize(fixtures, results)["tree"]
        self.assertEqual(result["correct"], 1)
        self.assertEqual(result["mean_score"], 0.0)


if __name__ == "__main__":
    unittest.main()
