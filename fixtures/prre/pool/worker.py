"""Controlled runtime fixture. Execute only in an authorized isolated environment."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from threading import Barrier, BoundedSemaphore
import tomllib


def configuration():
    return tomllib.loads(Path("osmogrep.runtime.toml").read_text())


def acquire_once():
    pool = BoundedSemaphore(configuration()["resources"][0]["capacity"])
    acquired = pool.acquire(blocking=False)
    if acquired:
        pool.release()
    return acquired


def run_batch():
    config = configuration()
    concurrency = config["services"][0]["concurrency"]
    pool = BoundedSemaphore(config["resources"][0]["capacity"])
    attempted = Barrier(concurrency)

    def work(_):
        acquired = pool.acquire(blocking=False)
        try:
            attempted.wait(timeout=5)
            return acquired
        finally:
            if acquired:
                pool.release()

    with ThreadPoolExecutor(max_workers=concurrency) as workers:
        return all(list(workers.map(work, range(concurrency))))
