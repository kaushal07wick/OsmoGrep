"""Offline adapter contracts against the pinned Harbor SDK; never calls a model."""
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from harbor.models.agent.context import AgentContext
from benchmarks.harbor.osmogrep import OsmoGrepAgent


class Environment:
    default_user = None

    def __init__(self, return_code=0):
        self.commands = []
        self.uploads = {}
        self.return_code = return_code

    async def exec(self, **kwargs):
        subprocess.run(["bash", "-n"], input=kwargs["command"], text=True, check=True)
        self.commands.append(kwargs)
        return SimpleNamespace(return_code=self.return_code, stdout="", stderr="deliberate fixture failure")

    async def upload_file(self, source, target):
        self.uploads[target] = Path(source).read_text()

    async def download_file(self, source, target):
        usage = {"type": "usage", "usage": {"input": 120, "output": 30, "cached": 40}}
        Path(target).write_text(json.dumps(usage) + "\n{interrupted" if source.endswith("events.jsonl") else "fixture stderr")


class AdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_routes_each_wire_protocol_and_preserves_instruction_without_shell_interpolation(self):
        for model, protocol in [("openai/test-model", "responses"), ("anthropic/test-model", "anthropic-messages"), ("openrouter/vendor/test-model", "chat-completions")]:
            with self.subTest(model=model), tempfile.TemporaryDirectory() as folder:
                with patch.dict("os.environ", {"OSMOGREP_API_KEY": "fixture-secret", "LITELLM_LOCAL_MODEL_COST_MAP": "True"}, clear=True):
                    agent = OsmoGrepAgent(logs_dir=Path(folder), model_name=model, base_url="http://fixture.invalid/v1")
                    env = Environment()
                    context = AgentContext()
                    prompt = "Fix 'quotes'\n$(touch /tmp/should-not-exist); `echo bad`"
                    await agent.run(prompt, env, context)
                call = env.commands[-1]
                self.assertEqual(call["env"]["OSMOGREP_API"], protocol)
                self.assertEqual(call["env"]["OSMOGREP_API_KEY"], "fixture-secret")
                self.assertEqual(call["env"]["OSMOGREP_BASE_URL"], "http://fixture.invalid/v1")
                self.assertEqual(call["env"]["OSMOGREP_MODEL"], model.split("/", 1)[1])
                self.assertNotIn("fixture-secret", call["command"])
                self.assertNotIn("touch", call["command"])
                self.assertEqual(list(env.uploads.values()), [prompt])
                self.assertEqual((context.n_input_tokens, context.n_output_tokens, context.n_cache_tokens), (120, 30, 40))
                self.assertIsNone(context.cost_usd)

    async def test_failed_agent_stays_failed_and_keeps_logs(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict("os.environ", {"OPENAI_API_KEY": "fixture", "LITELLM_LOCAL_MODEL_COST_MAP": "True"}, clear=True):
            agent = OsmoGrepAgent(logs_dir=Path(folder), model_name="openai/test-model")
            with self.assertRaises(RuntimeError):
                await agent.run("Fix the task", Environment(return_code=7), AgentContext())
            self.assertTrue((Path(folder) / "stderr.log").exists())

    async def test_installer_is_pinned_and_checks_archive_before_extracting(self):
        with tempfile.TemporaryDirectory() as folder:
            agent = OsmoGrepAgent(logs_dir=Path(folder), model_name="openai/test-model")
            env = Environment()
            await agent.install(env)
            command = env.commands[0]["command"]
            self.assertIn("version=0.3.8", command)
            self.assertLess(command.index("sha256sum --check"), command.index("tar -xzf"))

    def test_absent_usage_is_unknown_and_unsafe_release_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            agent = OsmoGrepAgent(logs_dir=Path(folder))
            context = AgentContext()
            agent.populate_context_post_run(context)
            self.assertIsNone(context.n_input_tokens)
            for kwargs in ({"version": "latest;bad"}, {"max_iterations": 0}):
                with self.assertRaises(ValueError):
                    OsmoGrepAgent(logs_dir=Path(folder), **kwargs)


if __name__ == "__main__":
    unittest.main()
