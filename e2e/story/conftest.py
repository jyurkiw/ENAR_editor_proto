"""The primary suite: one ordered story that writes the Grand Forks supplement through the
editor. Each step module (test_NN_*.py) builds on the ones before it; after a step passes,
the project folder is saved to <run>/snapshots/NN so a later run can start from it
(`--from-snapshot NN` skips steps up to NN)."""
import re
import shutil

import pytest
from playwright.sync_api import expect

from pages.ui import UI

FAILED = set()


def step_of(path):
    m = re.match(r'test_(\d\d)_', path.name)
    return m.group(1) if m else None


def pytest_collection_modifyitems(config, items):
    nn = config.getoption('--from-snapshot')
    if not nn:
        return
    skip = pytest.mark.skip(reason=f'before the snapshot after step {nn}')
    for item in items:
        s = step_of(item.path)
        if s and s <= nn:
            item.add_marker(skip)


@pytest.hookimpl(wrapper=True)
def pytest_runtest_makereport(item, call):
    rep = yield
    if rep.failed:
        FAILED.add(item.path)
    return rep


@pytest.fixture(scope='module', autouse=True)
def snapshot(request, app):
    yield
    nn = step_of(request.path)
    if nn and request.path not in FAILED and app.project_dir.exists():
        dest = app.run_dir / 'snapshots' / nn
        if dest.exists():
            shutil.rmtree(dest)
        shutil.copytree(app.project_dir, dest)


@pytest.fixture(scope='session')
def ui(app):
    return UI(app)


@pytest.fixture
def opened(ui, app):
    """The project is open (it is after step 01; a run from a snapshot opens it here)."""
    if not ui.page.locator('.tree .row[data-path]').count() and not ui.page.locator('.toolbar .project', has_text='grand-forks').count():
        app.queue_pick(app.project_dir)
        ui.page.locator('.toolbar').get_by_role('button', name='Open project…').click()
        ui.idle()
    expect(ui.page.locator('.toolbar .project')).to_have_text('grand-forks')
    return ui
