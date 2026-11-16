"""Independent checks for the small live coding fixtures, separate from agent claims."""
import ast
import os
import re
import subprocess


def run_check(root, command):
    env = {'PATH': os.environ.get('PATH', os.defpath), 'HOME': str(root)}
    return subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=10).returncode == 0


def documentation(root):
    text = (root / 'README.md').read_text()
    return (text.count('# Example\n') == 1 and text.count('USER_NOTE_KEEP_947') == 1
            and text.index('# Example\n') < text.index('USER_NOTE_KEEP_947')
            and re.search(r'^#{1,6}\s+installation\b', text, re.I | re.M) is not None
            and 'python3 -m pip install example' in text)


def bug_fix(root):
    # Extra inputs reject a hard-coded answer that happens to pass the visible test.
    behavior = 'from maths import mean; import math; samples=[[2,4,6],[-7,3],[0.25,0.75],[19],[0,0]]; assert all(math.isclose(mean(v),sum(v)/len(v)) for v in samples)'
    return run_check(root, ['python3', '-m', 'unittest', '-q']) and run_check(root, ['python3', '-c', behavior])


def multi_file_change(root):
    formatting = ast.parse((root / 'formatting.py').read_text())
    app = ast.parse((root / 'app.py').read_text())
    definitions = {node.name for node in ast.walk(formatting) if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))}
    imports = {alias.name for node in ast.walk(app) if isinstance(node, ast.ImportFrom) for alias in node.names}
    calls = {node.func.id for node in ast.walk(app) if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)}
    return ('format_greeting' in definitions and 'greeting' not in definitions
            and 'format_greeting' in imports and 'greeting' not in imports
            and 'format_greeting' in calls and 'greeting' not in calls
            and run_check(root, ['python3', '-m', 'unittest', '-q']))
