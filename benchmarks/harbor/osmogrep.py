"""Harbor installed-agent adapter; the OsmoGrep binary owns the entire agent loop."""
import json
import re
import shlex

from harbor.agents.agent_auth_resolution import ModelConnectionSpec, ProviderAccess
from harbor.agents.capabilities import AgentCapabilities
from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
from harbor.models.model_api_format import ModelApiFormat as API


class OsmoGrepAgent(BaseInstalledAgent):
    capabilities = AgentCapabilities(base_url=True, reasoning_effort=True)
    MODEL_CONNECTION = ModelConnectionSpec(
        default_provider="openai",
        agent_provider_access=ProviderAccess(
            api_key_candidate_names=["OSMOGREP_API_KEY"],
            base_url_candidate_names=["OSMOGREP_BASE_URL"],
        ),
        supported_api_formats=[API.OPENAI_RESPONSES, API.OPENAI_CHAT_COMPLETIONS, API.ANTHROPIC_MESSAGES],
        default_api_format_by_provider={
            "openai": API.OPENAI_RESPONSES,
            "anthropic": API.ANTHROPIC_MESSAGES,
            "default": API.OPENAI_CHAT_COMPLETIONS,
        },
        api_key_destination_names={"default": "OSMOGREP_API_KEY"},
        base_url_destination_names={"default": "OSMOGREP_BASE_URL"},
    )

    @staticmethod
    def name():
        return "osmogrep"

    def __init__(self, *args, version="0.3.8", max_iterations=90, reasoning_effort=None, **kwargs):
        if not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError("version must be a pinned release such as 0.3.8")
        self.max_iterations = int(max_iterations)
        if not 1 <= self.max_iterations <= 10000:
            raise ValueError("max_iterations must be between 1 and 10000")
        if reasoning_effort not in (None, "none", "low", "medium", "high", "xhigh"):
            raise ValueError("unsupported reasoning_effort")
        self.reasoning_effort = reasoning_effort
        super().__init__(*args, version=version, **kwargs)

    async def install(self, environment):
        # Release binaries currently support x86_64 Linux task containers.
        await self.exec_as_root(environment, """set -eu
test "$(uname -m)" = x86_64
command -v apt-get >/dev/null
if [ "$(printf '%s\n' 2.39 "$(getconf GNU_LIBC_VERSION | cut -d ' ' -f 2)" | sort -V | head -n 1)" != 2.39 ]; then
    echo 'OsmoGrep release requires glibc 2.39+; use a compatible prebuilt image/binary and skip_install=true.' >&2
    exit 1
fi
apt-get update -qq
apt-get install -y -qq ca-certificates curl tar git ripgrep coreutils
apt-get install -y -qq libasound2t64 || apt-get install -y -qq libasound2
"""
            + f"version={shlex.quote(self._version)}\n"
            + """archive=osmogrep-x86_64-unknown-linux-gnu.tar.gz
base=https://github.com/kaushal07wick/OsmoGrep/releases/download/v$version
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
curl --fail --location --retry 3 --output "$archive" "$base/$archive"
curl --fail --location --retry 3 --output "$archive.sha256" "$base/$archive.sha256"
sha256sum --check "$archive.sha256"
tar -xzf "$archive" --no-same-owner osmogrep
install -m 755 osmogrep /usr/local/bin/osmogrep
osmogrep --version
""")

    @with_prompt_template
    async def run(self, instruction, environment, context):
        connection = self.model_connection
        if not connection.model_id:
            raise ValueError("select an explicit provider/model with -m")
        api = {
            API.OPENAI_RESPONSES: "responses",
            API.OPENAI_CHAT_COMPLETIONS: "chat-completions",
            API.ANTHROPIC_MESSAGES: "anthropic-messages",
        }[connection.model_api_format]
        env = {
            **connection.env,
            "OSMOGREP_PROVIDER": connection.provider,
            "OSMOGREP_MODEL": connection.model_id,
            "OSMOGREP_API": api,
            "OSMOGREP_MAX_ITERATIONS": str(self.max_iterations),
            "XDG_CONFIG_HOME": str(self.environment_logs_dir / "config"),
        }
        if connection.api_key is not None:
            env["OSMOGREP_API_KEY"] = connection.api_key
        if connection.execution_base_url is not None:
            env["OSMOGREP_BASE_URL"] = connection.execution_base_url
        if self.reasoning_effort is not None:
            env["OSMOGREP_REASONING_EFFORT"] = self.reasoning_effort
        prompt = str(self.environment_logs_dir / "instruction.txt")
        await self._upload_config_text(environment, content=instruction, remote_path=prompt, filename="instruction.txt")
        command = (
            f"osmogrep run --repo-root . --prompt-file {shlex.quote(prompt)} "
            "--json-events --permission-profile full-access --auto-approve "
            f"> {shlex.quote(str(self.environment_logs_dir / 'events.jsonl'))} "
            f"2> {shlex.quote(str(self.environment_logs_dir / 'stderr.log'))}"
        )
        try:
            await self.exec_as_agent(environment, command, env=env)
        finally:
            await environment.download_file(str(self.environment_logs_dir / "events.jsonl"), self.logs_dir / "events.jsonl")
            await environment.download_file(str(self.environment_logs_dir / "stderr.log"), self.logs_dir / "stderr.log")
            self.populate_context_post_run(context)

    def populate_context_post_run(self, context):
        path = self.logs_dir / "events.jsonl"
        if not path.exists():
            return
        usage = []
        for line in path.read_text().splitlines():
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue  # An interrupted process can leave its last line incomplete.
            if event.get("type") == "usage":
                usage.append(event["usage"])
        if usage:
            context.n_input_tokens = sum(u["input"] for u in usage)
            context.n_output_tokens = sum(u["output"] for u in usage)
            context.n_cache_tokens = sum(u["cached"] for u in usage)
        # The provider does not report a price; unknown cost must remain unknown.
