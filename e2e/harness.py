"""Building, launching and attaching to the editor's e2e build. Used by conftest.py and by
tools/launch.py (a long-lived app to write tests against with `pytest --attach`)."""
import json
import os
import socket
import subprocess
import time
import urllib.request
from pathlib import Path

E2E = Path(__file__).resolve().parent
PROTO_UI = E2E.parent
TARGET = PROTO_UI / 'target' / 'e2e'          # its own target dir: a running `cargo tauri dev` can't lock it
EXE = TARGET / 'debug' / 'enigma-editor.exe'
LIVE = E2E / 'out' / 'live'                   # tools/launch.py's run folder
PASSED = set()                                # "test_NN_x.py::test_y" for each story test that passed


class BuildFailed(Exception):
    pass


def build():
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    env.pop('NO_COLOR', None)
    r = subprocess.run(['cargo', 'tauri', 'build', '--debug', '--no-bundle', '--features', 'e2e'],
                       cwd=PROTO_UI, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace')
    if r.returncode != 0:
        raise BuildFailed(f'{r.stdout[-4000:]}\n{r.stderr[-4000:]}')


def free_port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def cdp_up(port):
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/version', timeout=1) as r:
            return json.load(r)
    except OSError:
        return None


def launch(run_dir, port=None, env=None):
    """Starts the e2e build with its own settings, WebView2 profile and change log in run_dir
    (and any `env` on top). Returns (process, port) once WebView2's debugging port answers."""
    run_dir = Path(run_dir)
    run_dir.mkdir(parents=True, exist_ok=True)
    port = port or free_port()
    extra = env
    env = dict(os.environ,
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={port}',
               WEBVIEW2_USER_DATA_FOLDER=str(run_dir / 'webview'),
               ENIGMA_CONFIG_DIR=str(run_dir / 'config'),
               ENIGMA_E2E_LOG=str(run_dir / 'changes.jsonl'))
    env.update(extra or {})
    log = open(run_dir / 'app.log', 'w', encoding='utf-8')
    proc = subprocess.Popen([str(EXE)], cwd=run_dir, env=env, stdout=log, stderr=subprocess.STDOUT)
    proc.log = log
    end = time.time() + 60
    while time.time() < end:
        if proc.poll() is not None:
            raise RuntimeError(f'the editor exited with {proc.returncode} before WebView2 started (see {run_dir / "app.log"})')
        if cdp_up(port):
            (run_dir / 'port').write_text(str(port))
            return proc, port
        time.sleep(0.25)
    proc.kill()
    raise TimeoutError(f'WebView2 never opened its debugging port {port}')


def stop(proc):
    proc.terminate()
    try:
        proc.wait(10)
    except subprocess.TimeoutExpired:
        proc.kill()
    proc.log.close()


def app_venv():
    """The Python environment the app runs the validator and compiler in (its cache folder,
    UV_PROJECT_ENVIRONMENT), so a compile here doesn't build one of its own."""
    return Path(os.environ['LOCALAPPDATA']) / 'com.enigmanarrative.editor' / 'venv'


def compile_supplement(data_repo, supplement, out_pdf):
    """Runs the compiler the way the app does (uv run --project <compiler>, the app's venv)
    and returns (exit code, its output)."""
    compiler = Path(data_repo) / 'compiler'
    env = dict(os.environ, UV_PROJECT_ENVIRONMENT=str(app_venv()), PYTHONIOENCODING='utf-8')
    r = subprocess.run(['uv', 'run', '--quiet', '--project', str(compiler), 'python', '-m', 'encompile', str(supplement), '-o', str(out_pdf)],
                       cwd=compiler, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace')
    return r.returncode, r.stdout + r.stderr


def app_page(browser):
    """The editor's page, once the UI and the e2e hook are up."""
    page = next(p for c in browser.contexts for p in c.pages if not p.url.startswith('devtools'))
    page.wait_for_function('() => window.__enigmaE2E && document.body.children.length > 0', timeout=30_000)
    return page


class App:
    """The running editor: its page, its folders, and the e2e seams."""

    def __init__(self, page, run_dir, project_dir, data_repo):
        self.page = page
        self.run_dir = Path(run_dir)
        self.project_dir = Path(project_dir)
        self.data_repo = Path(data_repo)
        self.change_log = self.run_dir / 'changes.jsonl'

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
        text = self.change_log.read_text(encoding='utf-8')
        # The app may be part way through writing the last line: only complete lines count.
        done = text[:text.rfind('\n') + 1]
        return [json.loads(line) for line in done.splitlines() if line]
