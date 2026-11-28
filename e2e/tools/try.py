"""Runs a Python snippet against the live app (tools/launch.py), with `ui`, `app`, `page`
and `expect` in scope: for trying selectors while writing a step.

    uv run python tools/try.py snippet.py
    uv run python tools/try.py -c "ui.select('chapter', name='tollside')"
"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from playwright.sync_api import expect, sync_playwright  # noqa: E402

import harness  # noqa: E402
from conftest import project_path  # noqa: E402
from pages.ui import UI  # noqa: E402

if __name__ == '__main__':
    code = sys.argv[2] if sys.argv[1] == '-c' else Path(sys.argv[1]).read_text(encoding='utf-8')
    port = int((harness.LIVE / 'port').read_text())
    data_repo = Path(os.environ.get('ENIGMA_DATA_REPO', 'E:/Data/enigma-narrative'))
    with sync_playwright() as pw:
        page = harness.app_page(pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}'))
        app = harness.App(page, harness.LIVE, project_path(data_repo), data_repo)
        ui = UI(app)
        exec(compile(code, 'snippet', 'exec'), {'ui': ui, 'app': app, 'page': page, 'expect': expect})
