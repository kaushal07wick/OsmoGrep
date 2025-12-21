#!/usr/bin/env python3
import sys
import subprocess

def main():
    # Read everything from stdin (prompt text)
    prompt = sys.stdin.read()
    if not prompt.strip():
        return

    # Change model here if needed
    model = "qwen2.5-coder:7b"

    # Call Ollama
    proc = subprocess.run(
        ["ollama", "run", model],
        input=prompt,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    if proc.returncode != 0:
        # Forward Ollama error to stderr
        sys.stderr.write(proc.stderr)
        sys.exit(proc.returncode)

    # Output ONLY model response
    sys.stdout.write(proc.stdout)

if __name__ == "__main__":
    main()
