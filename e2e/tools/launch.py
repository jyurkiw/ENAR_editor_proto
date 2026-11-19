"""Starts the e2e build and keeps it open, for writing tests: run steps against it with
`uv run pytest story -k 05 --attach`, and look at it yourself while they run.

    uv run python tools/launch.py            (from the e2e folder; Ctrl+C closes the app)
    uv run python tools/launch.py --build    (build first)

Its run folder is e2e/out/live. The project is left as it is, so --attach runs pick up where
the last one stopped; delete <data repo>/adventures/e2e/grand-forks to start again.
"""
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import harness  # noqa: E402

if __name__ == '__main__':
    if '--build' in sys.argv:
        print('building…', flush=True)
        harness.build()
    proc, port = harness.launch(harness.LIVE)
    print(f'editor running, CDP on port {port}; Ctrl+C to close', flush=True)
    try:
        while proc.poll() is None:
            time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    finally:
        harness.stop(proc)
