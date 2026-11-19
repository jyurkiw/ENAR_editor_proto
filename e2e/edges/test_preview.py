"""The Preview panel: a burst of changes renders once (the last one); while it renders, the
last picture stays up, faded; and without pdflatex it says so instead of going blank."""
import os
import re

from playwright.sync_api import expect

import harness
from pages.ui import UI

PIKE = 'grand-forks/forkjaw-pike.xml'

# Counts renders: each one shows "Rendering…" while it runs.
COUNT = """() => {
    window.__renders = 0;
    if (window.__renderWatch) window.__renderWatch.disconnect();
    window.__renderWatch = new MutationObserver(ms => {
        for (const m of ms) for (const n of m.addedNodes)
            if (n.nodeType === 1 && (n.matches('.preview-busy') || n.querySelector('.preview-busy'))) window.__renders++;
    });
    window.__renderWatch.observe(document.body, {subtree: true, childList: true});
}"""


def stat_rows(ui):
    ui.side('File structure')
    return ui.page.locator('.tree .row.element[data-tag="monster"]')


def wait_rendered(ui):
    panel = ui.page.locator('.preview-panel')
    expect(panel.locator('.preview-busy')).to_have_count(0, timeout=120_000)
    expect(panel.locator('.preview-img').first).to_be_visible(timeout=120_000)
    return panel


def test_it_renders_only_what_it_shows(edge):
    """The panel watches only while it's showing: three stat blocks picked with it hidden
    print nothing, and showing it again prints the last one, once."""
    ui = edge
    ui.open(PIKE)
    stat_rows(ui).first.click()
    ui.side('Preview')
    wait_rendered(ui)
    ui.page.evaluate(COUNT)
    rows = stat_rows(ui)
    for i in [1, 0, 1]:
        rows.nth(i).click()
    assert ui.page.evaluate('() => window.__renders') == 0
    ui.side('Preview')
    panel = ui.page.locator('.preview-panel')
    expect(panel.locator('.preview-name')).to_have_text('Forkjaw Matriarch')
    ui.page.wait_for_function('() => window.__renders > 0', timeout=10_000)
    wait_rendered(ui)
    ui.page.wait_for_timeout(500)
    assert ui.page.evaluate('() => window.__renders') == 1


def test_the_last_picture_stays_up_faded(edge):
    ui = edge
    ui.open(PIKE)
    stat_rows(ui).first.click()
    ui.side('Preview')
    panel = wait_rendered(ui)
    before = panel.locator('.preview-img').first.get_attribute('src')
    # Watch for the stale state from the page itself: it can be short.
    ui.page.evaluate("""() => {
        window.__sawStale = false;
        const seen = () => {
            const p = document.querySelector('.preview-pages');
            if (p && p.classList.contains('stale') && p.querySelector('img')) window.__sawStale = true;
        };
        window.__staleWatch = new MutationObserver(seen);
        window.__staleWatch.observe(document.body, {subtree: true, attributes: true, childList: true});
    }""")
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    sec = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Heading')))
    card = sec.locator('.table-card').first
    ac = ui.field('Armor Class', scope=card)
    ac.fill('15' if ac.input_value() != '15' else '16')
    ui.apply(card)
    ui.side('Preview')
    panel = wait_rendered(ui)
    expect(panel.locator('.preview-img').first).not_to_have_attribute('src', before, timeout=120_000)
    assert ui.page.evaluate('() => { window.__staleWatch.disconnect(); return window.__sawStale; }')


def test_without_pdflatex(app, edge):
    """A second editor, with TeX taken off its PATH, on the same project."""
    path = os.pathsep.join(p for p in os.environ['PATH'].split(os.pathsep)
                           if not re.search(r'texlive|miktex|tex\\bin', p, re.I))
    run = app.run_dir / 'no-pdflatex'
    proc, port = harness.launch(run, env={'PATH': path})
    try:
        # Through the Playwright the session already runs (a second one can't start in it).
        browser = app.page.context.browser.browser_type.connect_over_cdp(f'http://127.0.0.1:{port}')
        try:
            page = harness.app_page(browser)
            other = harness.App(page, run, app.project_dir, app.data_repo)
            ui = UI(other)
            other.queue_pick(app.project_dir)
            page.locator('.toolbar').get_by_role('button', name='Open project…').click()
            ui.idle()
            ui.open(PIKE)
            ui.side('Preview')
            err = page.locator('.preview-panel .preview-error')
            expect(err).to_be_visible(timeout=120_000)
            expect(err).to_contain_text(re.compile('pdflatex', re.I))
            assert page.locator('.preview-panel .preview-img').count() == 0
        finally:
            browser.close()
    finally:
        harness.stop(proc)
