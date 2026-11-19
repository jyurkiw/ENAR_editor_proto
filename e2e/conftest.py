"""Builds the editor with its e2e test seams, launches it, and hands tests a Playwright page.

Playwright can't launch a WebView2 app, so the app is started with WebView2's remote
debugging port open and Playwright attaches over CDP. Each run gets its own folder under
e2e/out/ (settings, WebView2 profile, app log, change log), so it never sees your own
last project. The test project is <data repo>/adventures/e2e/grand-forks, emptied at the start.

    uv run pytest                 (from the e2e folder; builds first)
    uv run pytest --no-build      (reuse the last e2e build)
"""
import json
import os
import shutil
import socket
import subprocess
import time
import urllib.request
from datetime import datetime
from pathlib import Path

import pytest
from playwright.sync_api import sync_playwright

E2E = Path(__file__).resolve().parent
PROTO_UI = E2E.parent
TARGET = PROTO_UI / 'target' / 'e2e'          # its own target dir: a running `cargo tauri dev` can't lock it
EXE = TARGET / 'debug' / 'enigma-editor.exe'


def pytest_addoption(parser):
    parser.addoption('--no-build', action='store_true', help='use the last e2e build as it is')
    parser.addoption('--data-repo', default=os.environ.get('ENIGMA_DATA_REPO', 'E:/Data/enigma-narrative'),
                     help='the enigma-narrative checkout (schemas, compiler, shared files)')


def build():
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    env.pop('NO_COLOR', None)
    r = subprocess.run(['cargo', 'tauri', 'build', '--debug', '--no-bundle', '--features', 'e2e'],
                       cwd=PROTO_UI, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace')
    if r.returncode != 0:
        pytest.exit(f'the e2e build failed:\n{r.stdout[-4000:]}\n{r.stderr[-4000:]}', returncode=3)


def free_port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def wait_for_cdp(port, proc, timeout=60):
    end = time.time() + timeout
    while time.time() < end:
        if proc.poll() is not None:
            raise RuntimeError(f'the editor exited with {proc.returncode} before WebView2 started')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1) as r:
                return json.load(r)
        except OSError:
            time.sleep(0.25)
    raise TimeoutError(f'WebView2 never opened its debugging port {port}')


class App:
    """The running editor: its page, its folders, and the e2e seams."""

    def __init__(self, page, run_dir, project_dir, data_repo):
        self.page = page
        self.run_dir = run_dir
        self.project_dir = project_dir
        self.data_repo = data_repo
        self.change_log = run_dir / 'changes.jsonl'

    def invoke(self, cmd, args=None):
        return self.page.evaluate('([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a)', [cmd, args or {}])

    def queue_pick(self, *paths):
        """Answers for the next folder pickers, in order. None answers Cancel."""
        self.invoke('e2e_queue_pick', {'paths': ['' if p is None else str(p) for p in paths]})

    def answer_confirms(self, *answers):
        """Answers for the next confirm dialogs, in order (OK once these run out)."""
        self.page.evaluate('a => window.__enigmaE2E.answers.push(...a)', [bool(a) for a in answers])

    def confirms_asked(self):
        """The confirm messages shown since the last call."""
        return self.page.evaluate('() => window.__enigmaE2E.asked.splice(0)')

    def changes(self):
        """Every apply so far: {"change": ..., "result": {"status": ...}}."""
        if not self.change_log.exists():
            return []
        return [json.loads(line) for line in self.change_log.read_text(encoding='utf-8').splitlines() if line]


@pytest.fixture(scope='session')
def run_dir():
    d = E2E / 'out' / datetime.now().strftime('%Y%m%d-%H%M%S')
    d.mkdir(parents=True)
    return d


@pytest.fixture(scope='session')
def data_repo(pytestconfig):
    d = Path(pytestconfig.getoption('--data-repo')).resolve()
    if not (d / 'schemas' / 'supplement-template-schema.xsd').is_file():
        pytest.exit(f'{d} is not an enigma-narrative checkout (use --data-repo)', returncode=4)
    return d


@pytest.fixture(scope='session')
def project_dir(data_repo):
    """<data repo>/adventures/e2e/grand-forks, empty. adventures/e2e ignores itself in git."""
    base = data_repo / 'adventures' / 'e2e'
    base.mkdir(parents=True, exist_ok=True)
    ignore = base / '.gitignore'
    if not ignore.exists():
        ignore.write_text('# Written by the editor\'s e2e tests (proto_ui/e2e); not part of the repo.\n*\n')
    d = base / 'grand-forks'
    if d.exists():
        shutil.rmtree(d)
    d.mkdir()
    return d


@pytest.fixture(scope='session')
def app(pytestconfig, run_dir, project_dir, data_repo):
    if not pytestconfig.getoption('--no-build'):
        build()
    if not EXE.is_file():
        pytest.exit(f'{EXE} is missing: run without --no-build', returncode=3)

    port = free_port()
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(run_dir / 'webview'),
               ENIGMA_CONFIG_DIR=str(run_dir / 'config'),
               ENIGMA_E2E_LOG=str(run_dir / 'changes.jsonl'))
    log = open(run_dir / 'app.log', 'w', encoding='utf-8')
    proc = subprocess.Popen([str(EXE)], cwd=run_dir, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        wait_for_cdp(port, proc)
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}')
            page = next(p for c in browser.contexts for p in c.pages if not p.url.startswith('devtools'))
            page.wait_for_function('() => window.__enigmaE2E && document.body.children.length > 0', timeout=30_000)
            yield App(page, run_dir, project_dir, data_repo)
            browser.close()
    finally:
        proc.terminate()
        try:
            proc.wait(10)
        except subprocess.TimeoutExpired:
            proc.kill()
        log.close()


@pytest.fixture
def page(app):
    return app.page
