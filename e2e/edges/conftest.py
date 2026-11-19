"""The secondary suite: independent tests of behaviour that would muddy the story. Each
module starts from the story's last snapshot (this run's, or the newest run's), with every
tab closed and the project opened again."""
import shutil

import pytest
from playwright.sync_api import expect

import harness
from pages.ui import UI


def last_snapshot(run_dir):
    here = sorted((run_dir / 'snapshots').glob('1[0-2]')) if (run_dir / 'snapshots').exists() else []
    if here:
        return here[-1]
    runs = sorted(harness.E2E.glob('out/2*/snapshots/12'))
    return runs[-1] if runs else None


@pytest.fixture(scope='module')
def edge(app):
    """The project as the story left it, freshly opened."""
    snap = last_snapshot(app.run_dir)
    if snap is None:
        pytest.skip('no story snapshot yet: run the story once')
    ui = UI(app)
    for close in ui.page.locator('.file-tab button.close').all()[::-1]:
        close.click()
    expect(ui.page.locator('.file-tab')).to_have_count(0)
    if app.project_dir.exists():
        shutil.rmtree(app.project_dir)
    shutil.copytree(snap, app.project_dir)
    app.queue_pick(app.project_dir)
    ui.page.locator('.toolbar').get_by_role('button', name='Open project…').click()
    ui.idle()
    expect(ui.page.locator('.toolbar .project')).to_have_text('grand-forks')
    app.confirms_asked()
    return ui
