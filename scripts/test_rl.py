"""CPU-only regression tests: python3 -m unittest discover -s scripts -p 'test_*.py'."""

import argparse
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

import rl_common
import rl_prepare
import rl_probe
import rl_train
import sft_train


def call(name="submit_script", arguments=None):
    return "<tool_call>" + json.dumps({"name": name, "arguments": arguments}) + "</tool_call>"


class RolloutTests(unittest.TestCase):
    def test_reward_rejects_a_complete_tool_call_in_a_truncated_rollout(self):
        text = '<think>work</think>' + call(arguments={'script': '51'})
        with patch.object(rl_train, 'completion_end_ids', {2, 0}), patch.object(
            rl_train.urllib.request, 'urlopen', return_value=io.BytesIO(b'[{"shaped":0},{"shaped":1}]')
        ) as request:
            self.assertEqual(rl_train.oracle_reward([text, text], ['{"task":"write"}'] * 2,
                thinking=[True, True], completion_ids=[[5, 6], [5, 2]]), [0, 1])
            items = json.loads(request.call_args.args[0].data)['items']
            self.assertEqual(items[0]['answer'], '')
            self.assertEqual(items[1]['answer'], {'task':'script', 'script':'51'})
    def test_all_task_kinds_select_the_correct_tool(self):
        for prefix, kind in rl_prepare.KINDS.items():
            self.assertEqual(rl_prepare.kind_of({"id": prefix + "-0"}), kind)
            expected = {"identify": "submit_identify", "tree": "submit_descriptor"}.get(kind, "submit_script")
            self.assertEqual(rl_prepare.tool_for(kind)["function"]["name"], expected)
        self.assertEqual(rl_prepare.kind_of({"id": "human-test", "task": "write"}), "write")
        with self.assertRaises(ValueError):
            rl_prepare.kind_of({"id": "t6-0"})
        with self.assertRaises(ValueError):
            rl_prepare.tool_for("unknown")

    def test_final_tool_extraction_rejects_reasoning_and_malformed_arguments(self):
        good = call(arguments={"script": "51"})
        self.assertEqual(rl_common.extract_answer(good), {"task": "script", "script": "51"})
        self.assertEqual(rl_common.extract_answer(good, thinking=True), "")
        self.assertEqual(rl_common.extract_answer(good, finish_reason="length"), "")
        self.assertEqual(rl_common.extract_answer("<think>" + good), "")
        final = "<think>" + good + "</think>" + call(arguments={"script": "00"})
        self.assertEqual(rl_common.extract_answer(final)["script"], "00")
        for bad in [None, [], 7, {"script": None}, "bad json"]:
            self.assertEqual(rl_common.extract_answer(call(arguments=bad) + good)["script"], "51")
        self.assertEqual(rl_common.extract_answer(call(arguments=json.dumps({"script": "51"})))["script"], "51")

    def test_thinking_and_contract_mismatches_fail_before_training(self):
        row = {"thinking": True, "task_json": json.dumps({"task": "judgment", "contract_version": 1})}
        rl_common.validate_rows([row], True)
        for rows, thinking in [([], True), ([row], False),
                               ([{"task_json": row["task_json"]}], True),
                               ([dict(row, task_json='{"task":"judgment"}')], True)]:
            with self.assertRaises(ValueError):
                rl_common.validate_rows(rows, thinking)

    def test_probe_stream_preserves_all_choices_and_is_uncapped_by_default(self):
        parser = argparse.ArgumentParser()
        rl_common.add_sampling_args(parser)
        args = parser.parse_args(["--k", "2"])
        args.seed = 7
        events = [
            {"choices": [{"index": 1, "text": "second", "finish_reason": None}]},
            {"choices": [{"index": 0, "text": "first", "finish_reason": "stop"}]},
            {"choices": [{"index": 1, "text": " answer", "finish_reason": "stop"}]},
        ]
        data = "".join("data: " + json.dumps(e) + "\n\n" for e in events) + "data: [DONE]\n"
        with patch.object(rl_probe.urllib.request, "urlopen", return_value=io.BytesIO(data.encode())) as request:
            result = rl_probe.sample("http://test/v1", "model",
                                     {"messages": [], "tools": [], "thinking": True}, args)
            payload = json.loads(request.call_args.args[0].data)
            self.assertNotIn("max_tokens", payload)
            self.assertNotIn("max_completion_tokens", payload)
            self.assertTrue(request.call_args.args[0].full_url.endswith("/chat/completions"))
            self.assertNotIn("timeout", request.call_args.kwargs)
            self.assertEqual(payload["temperature"], rl_common.TEMPERATURE)
            self.assertEqual(payload["top_k"], rl_common.TOP_K)
            self.assertEqual(payload["min_p"], rl_common.MIN_P)
            self.assertEqual(payload["seed"], 7)
            self.assertEqual([c["text"] for c in result], ["first", "second answer"])

    def test_chat_stream_reassembles_tool_arguments_and_keeps_reasoning(self):
        args = argparse.Namespace(k=2, temperature=0.6, top_p=0.95, top_k=20, min_p=0.0, seed=7, max_completion_length=None)
        events = [
            {"choices": [{"index": 0, "delta": {"reasoning_content": "work"}}]},
            {"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function":
                {"name": "submit_script", "arguments": '{"script":'}}]}}]},
            {"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function":
                {"arguments": '"51"}'}}]}, "finish_reason": "tool_calls"}]},
            {"choices": [{"index": 1, "delta": {"content": "unfinished"}, "finish_reason": "length"}]},
        ]
        data = "".join("data: " + json.dumps(e) + "\n\n" for e in events) + "data: [DONE]\n"
        with patch.object(rl_probe.urllib.request, "urlopen", return_value=io.BytesIO(data.encode())):
            choices = rl_probe.sample("http://test/v1", "model",
                                      {"messages": [], "tools": [], "thinking": True}, args)
        self.assertEqual(choices[0]["reasoning"], "work")
        self.assertEqual(len(choices[0]["raw_chunks"]), 3)
        self.assertEqual(rl_probe.answer_from_sample(choices[0], True), {"task": "script", "script": "51"})
        self.assertEqual(rl_probe.answer_from_sample(choices[1], True), "")

    def test_incomplete_stream_keeps_partial_output_for_diagnosis(self):
        args = argparse.Namespace(k=2, temperature=0.6, top_p=0.95, top_k=20, min_p=0.0, seed=7, max_completion_length=None)
        data = b'data: {"choices":[{"index":0,"delta":{"content":"partial"}}]}\n'
        with patch.object(rl_probe.urllib.request, "urlopen", return_value=io.BytesIO(data)):
            with self.assertRaises(rl_probe.SampleError) as caught:
                rl_probe.sample("http://test/v1", "model",
                                {"messages": [], "tools": [], "thinking": True}, args)
        self.assertEqual(caught.exception.completions[0]["text"], "partial")

    def test_trainers_forward_resume_and_sampling_settings(self):
        for module, trainer_name, config_name in [(sft_train, "SFTTrainer", "SFTConfig"),
                                                   (rl_train, "GRPOTrainer", "GRPOConfig")]:
            captured = {}
            class Trainer:
                def __init__(self, **kwargs):
                    captured["trainer"] = kwargs
                    self.processing_class = types.SimpleNamespace(eos_token_id=2, pad_token_id=0)
                def train(self, **kwargs): captured["train"] = kwargs
                def save_model(self, path): captured["saved"] = path
            def config(**kwargs):
                captured["config"] = kwargs
                return kwargs
            row = {"thinking": True, "task_json": '{"task":"write"}'}
            modules = {"datasets": types.SimpleNamespace(load_dataset=lambda *a, **k: [row]),
                       "peft": types.SimpleNamespace(LoraConfig=lambda **k: k),
                       "trl": types.SimpleNamespace(**{trainer_name: Trainer, config_name: config})}
            args = [module.__file__, "--resume-from-checkpoint", "runs/checkpoint-200", "--warmup-steps", "3"]
            if module is rl_train:
                args += ["--temperature", "0.8", "--k", "4", "--reward-url", "http://test/reward/batch"]
            with patch.dict(sys.modules, modules), patch.object(sys, "argv", args):
                module.main()
            self.assertEqual(captured["train"]["resume_from_checkpoint"], "runs/checkpoint-200")
            self.assertEqual(captured["config"]["warmup_steps"], 3)
            if module is rl_train:
                self.assertEqual(captured["config"]["temperature"], 0.8)
                self.assertEqual(captured["config"]["num_generations"], 4)
                self.assertTrue(captured["config"]["mask_truncated_completions"])

    def test_prepare_refuses_evaluation_and_old_judgment_without_ml_dependencies(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)
            for manifest, fixture, expected in [
                ({"evaluation_only": True}, {}, "reserved for evaluation"),
                ({}, {"task": "judgment", "id": "t5-0"}, "Old judgment contract"),
            ]:
                (path / "manifest.json").write_text(json.dumps(manifest))
                (path / "fixtures.jsonl").write_text(json.dumps(fixture) + "\n")
                result = subprocess.run([sys.executable, "scripts/rl_prepare.py", "--pool", tmp],
                                        capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)

    def test_probe_regrade_uses_saved_completions_without_a_model(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, out = Path(tmp) / "raw.jsonl", Path(tmp) / "regraded.jsonl"
            record = {"row": {"task_id": "t5-0", "kind": "judgment", "thinking": False,
                              "task_json": '{"task":"judgment","contract_version":1}'},
                      "completions": [{"text": call(arguments={"script": "51"})},
                                      {"text": call(arguments={"script": "00"})}]}
            source.write_text(json.dumps(record) + "\n")
            with patch.object(sys, "argv", ["rl_probe", "--regrade", str(source), "--out", str(out)]), \
                 patch.object(rl_probe, "sample", side_effect=AssertionError("No model calls")), \
                 patch.object(rl_probe, "score", return_value=[{"shaped": 1}, {"shaped": 0}]):
                rl_probe.main()
            self.assertEqual(json.loads(out.read_text()), record)


if __name__ == "__main__":
    unittest.main()
