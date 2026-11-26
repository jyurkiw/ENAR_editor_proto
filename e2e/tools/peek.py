"""Runs a JavaScript expression in the live app (tools/launch.py) and prints the result.

    uv run python tools/peek.py "document.title"
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from playwright.sync_api import sync_playwright  # noqa: E402

import harness  # noqa: E402

if __name__ == '__main__':
    port = int((harness.LIVE / 'port').read_text())
    with sync_playwright() as pw:
        page = harness.app_page(pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}'))
        print(json.dumps(page.evaluate(sys.argv[1]), indent=1, ensure_ascii=False))
