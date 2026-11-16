import copy
import json
from pathlib import Path
import tempfile
import unittest

from harbor.models.job.config import JobConfig
from harbor.models.task.task import Task
from benchmarks.harbor.report import check_contract, report, trial_row

HERE = Path(__file__).parent


class EvaluationTests(unittest.TestCase):
    def test_pilot_is_content_pinned_and_inert_by_default(self):
        config = JobConfig.model_validate_json((HERE / "pilot.json").read_text())
        self.assertEqual([agent.name for agent in config.agents], ["nop"])
        self.assertEqual(len(config.tasks), 5)
        self.assertEqual(config.retry.max_retries, 0)
        self.assertEqual(config.n_attempts, 1)
        self.assertEqual(config.n_concurrent_trials, 1)
        self.assertEqual(len({task.name for task in config.tasks}), 5)
        for task in config.tasks:
            self.assertRegex(task.ref, r"^sha256:[0-9a-f]{64}$")
            self.assertIn("@sha256:", task.source)

    def test_contract_task_and_three_control_agents_validate_with_real_sdk(self):
        config = JobConfig.model_validate_json((HERE / "contract.json").read_text())
        self.assertEqual([a.name or a.import_path for a in config.agents], ["nop", "oracle", "benchmarks.harbor.osmogrep:OsmoGrepAgent"])
        task = Task(HERE / "tasks/adapter-contract")
        self.assertEqual(task.name, "adapter-contract")
        self.assertEqual(task.config.agent.timeout_sec, 60)

    def test_missing_grades_errors_and_nan_never_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = {"task_name": "task", "agent_info": {"name": "agent"}}
            for reward in (None, float("nan"), float("inf"), True, 2, "1"):
                row = trial_row({**base, "verifier_result": {"rewards": {"reward": reward}}}, root)
                self.assertEqual(row["outcome"], "ungraded")
                self.assertIsNone(row["input_tokens"])
                self.assertIsNone(row["cost_usd"])
                self.assertIsNone(row["tool_calls"])
            passing = {**base, "verifier_result": {"rewards": {"reward": 1}}}
            self.assertEqual(trial_row(passing, root)["outcome"], "pass")
            passing["exception_info"] = {"exception_type": "TimeoutError", "exception_message": "secret"}
            row = trial_row(passing, root)
            self.assertEqual(row["outcome"], "error")
            self.assertNotIn("secret", json.dumps(row))

    def test_metrics_preserve_cache_as_subset_and_ignore_agent_created_result_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            trial = root / "real-trial"
            agent = trial / "agent"
            agent.mkdir(parents=True)
            events = [{"type": "tool_call", "name": "read_file", "args": {"path": "a.py"}}] * 2 + [{"type": "done"}]
            (agent / "events.jsonl").write_text("\n".join(json.dumps(e) for e in events) + '\n{"interrupted')
            value = {"task_name": "real", "agent_info": {"name": "osmogrep"},
                     "agent_result": {"n_input_tokens": 120, "n_cache_tokens": 40, "n_output_tokens": 30},
                     "verifier_result": {"rewards": {"reward": 0}},
                     "config": {"agent": {"env": {"OPENAI_API_KEY": "secret"}, "kwargs": {"config": "secret", "version": "0.3.9"}}}}
            (trial / "result.json").write_text(json.dumps(value))
            (agent / "result.json").write_text(json.dumps({"task_name": "fake", "verifier_result": {"rewards": {"reward": 1}}}))
            summary = report(root)
            self.assertEqual(summary["attempts"], 1)
            row = summary["trials"][0]
            self.assertEqual((row["input_tokens"], row["cached_input_tokens"]), (120, 40))
            self.assertEqual((row["tool_calls"], row["read_calls"], row["unique_read_targets"]), (2, 2, 1))
            self.assertEqual(row["invalid_event_lines"], 1)
            self.assertTrue(row["completed_run_with_failed_grade"])
            self.assertNotIn("secret", json.dumps(summary))

    def test_contract_controls_reject_false_success_or_missing_tool_delivery(self):
        summary = {"trials": [{"agent": agent, "outcome": outcome, "tool_calls": 2, "input_tokens": 360, "output_tokens": 90}
                               for agent, outcome in [("nop", "fail"), ("oracle", "pass"), ("osmogrep", "pass")]]}
        check_contract(summary)
        bad = copy.deepcopy(summary)
        bad["trials"][0]["outcome"] = "pass"
        with self.assertRaises(ValueError):
            check_contract(bad)
        bad = copy.deepcopy(summary)
        bad["trials"][2]["tool_calls"] = 0
        with self.assertRaises(ValueError):
            check_contract(bad)


if __name__ == "__main__":
    unittest.main()
