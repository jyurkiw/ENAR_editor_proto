"""Builds the editor with its e2e test seams, launches it, and hands tests a Playwright page.

Playwright can't launch a WebView2 app, so the app is started with WebView2's remote
debugging port open and Playwright attaches over CDP. Each run gets its own folder under
e2e/out/ (settings, WebView2 profile, app log, change log, snapshots), so it never sees your
own last project. The test project is <data repo>/adventures/e2e/grand-forks.

    uv run pytest                          builds, then runs everything
    uv run pytest --no-build               reuses the last e2e build
    uv run pytest story --from-snapshot 03 runs the story from step 04, on step 03's files
    uv run pytest story -k 05 --attach     against the app tools/launch.py keeps open
"""
import shutil
from datetime import datetime
from pathlib import Path

import pytest
from playwright.sync_api import sync_playwright

import harness
from harness import App, E2E, EXE, LIVE


def pytest_addoption(parser):
    parser.addoption('--no-build', action='store_true', help='use the last e2e build as it is')
    parser.addoption('--attach', action='store_true',
                     help='use the app tools/launch.py started (no build, no launch, the project as it is)')
    parser.addoption('--from-snapshot', metavar='NN',
                     help='start the story after step NN, on the files the newest run saved after it')
    parser.addoption('--update-golden', action='store_true',
                     help="step 12 rewrites golden/ from this run's files instead of comparing them")
    parser.addoption('--data-repo', default=None,
                     help='the enigma-narrative checkout (default ENIGMA_DATA_REPO or E:/Data/enigma-narrative)')


def data_repo_of(config):
    import os
    return Path(config.getoption('--data-repo') or os.environ.get('ENIGMA_DATA_REPO', 'E:/Data/enigma-narrative')).resolve()


def snapshot_dir(nn):
    """The newest run's snapshot after story step nn."""
    runs = sorted((p for p in (E2E / 'out').glob('*/snapshots/' + nn) if p.is_dir()), key=lambda p: p.parent.parent.name)
    if not runs:
        raise pytest.UsageError(f'no run has a snapshot after step {nn}')
    return runs[-1]


@pytest.fixture(scope='session')
def run_dir(pytestconfig):
    if pytestconfig.getoption('--attach'):
        return LIVE
    d = E2E / 'out' / datetime.now().strftime('%Y%m%d-%H%M%S')
    d.mkdir(parents=True)
    return d


@pytest.fixture(scope='session')
def data_repo(pytestconfig):
    d = data_repo_of(pytestconfig)
    if not (d / 'schemas' / 'supplement-template-schema.xsd').is_file():
        pytest.exit(f'{d} is not an enigma-narrative checkout (use --data-repo)', returncode=4)
    return d


def project_path(data_repo):
    """<data repo>/adventures/e2e/grand-forks. adventures/e2e ignores itself in git."""
    base = data_repo / 'adventures' / 'e2e'
    base.mkdir(parents=True, exist_ok=True)
    ignore = base / '.gitignore'
    if not ignore.exists():
        ignore.write_text('# Written by the editor\'s e2e tests (proto_ui/e2e); not part of the repo.\n*\n')
    return base / 'grand-forks'


@pytest.fixture(scope='session')
def project_dir(pytestconfig, data_repo):
    """The test project: empty, or as a snapshot left it, or (--attach) as it is."""
    d = project_path(data_repo)
    if pytestconfig.getoption('--attach'):
        d.mkdir(exist_ok=True)
        return d
    if d.exists():
        shutil.rmtree(d)
    nn = pytestconfig.getoption('--from-snapshot')
    if nn:
        shutil.copytree(snapshot_dir(nn), d)
    else:
        d.mkdir()
    return d


@pytest.fixture(scope='session')
def app(pytestconfig, run_dir, project_dir, data_repo):
    attach = pytestconfig.getoption('--attach')
    proc = None
    if attach:
        port = int((LIVE / 'port').read_text())
        if not harness.cdp_up(port):
            pytest.exit('no live app: start one with `uv run python tools/launch.py`', returncode=3)
    else:
        if not pytestconfig.getoption('--no-build'):
            try:
                harness.build()
            except harness.BuildFailed as e:
                pytest.exit(f'the e2e build failed:\n{e}', returncode=3)
        if not EXE.is_file():
            pytest.exit(f'{EXE} is missing: run without --no-build', returncode=3)
        proc, port = harness.launch(run_dir)
    try:
        with sync_playwright() as pw:
            browser = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{port}')
            yield App(harness.app_page(browser), run_dir, project_dir, data_repo)
            browser.close()
    finally:
        if proc:
            harness.stop(proc)


@pytest.fixture
def page(app):
    return app.page


def pytest_collection_modifyitems(config, items):
    """The story first, then the secondary suite (edges/, which starts from the story's
    files), then the coverage check (coverage/, which needs both)."""
    items.sort(key=lambda i: 2 if 'coverage' in i.path.parts else 1 if 'edges' in i.path.parts else 0)


@pytest.hookimpl(wrapper=True)
def pytest_runtest_makereport(item, call):
    rep = yield
    if rep.when == 'call' and rep.passed:
        harness.PASSED.add(f'{item.path.name}::{item.originalname}')     # for the coverage check
    return rep
