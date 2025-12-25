import os
import sys
import subprocess

def main():
    prompt = sys.stdin.read()
    if not prompt.strip():
        return

    model = os.environ.get("OSMOGREP_OLLAMA_MODEL", "qwen2.5-coder:7b")

    proc = subprocess.run(
        ["ollama", "run", model],
        input=prompt,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        sys.exit(proc.returncode)

    sys.stdout.write(proc.stdout)

if __name__ == "__main__":
    main()
