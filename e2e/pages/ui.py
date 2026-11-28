"""Page objects: what the story steps do to the editor, in the editor's own words. The
selectors live here, so a markup change is fixed in one place."""
import re
import time
from pathlib import Path

from lxml import etree
from playwright.sync_api import Locator, Page, expect

TIMEOUT = 30_000          # an apply validates the whole file with lxml through uv


class UI:
    def __init__(self, app):
        self.app = app
        self.page: Page = app.page
        self.root: Path = app.project_dir

    # ----- waiting -----

    def idle(self):
        """No busy notice, no apply in flight."""
        expect(self.page.locator('.busy')).to_have_count(0, timeout=TIMEOUT)
        expect(self.page.get_by_role('button', name='Validating…')).to_have_count(0, timeout=TIMEOUT)

    def no_error(self):
        err = self.page.locator('.error-modal')
        if err.count():
            raise AssertionError('error dialog: ' + err.inner_text())

    # ----- side tabs -----

    def side(self, name):
        """"File system", "File structure" or "Preview"."""
        self.page.locator('.side-tab', has_text=name).click()

    # ----- the file system panel -----

    def path(self, rel=''):
        return str(self.root / rel) if rel else str(self.root)

    def file_row(self, rel='') -> Locator:
        p = self.path(rel)
        return self.page.locator(f'.tree .row[data-path="{css_str(p)}"]')

    def expand(self, rel=''):
        """Opens a folder in the file tree (and the folders above it)."""
        self.side('File system')
        parts = Path(rel).parts if rel else ()
        for i in range(len(parts) + 1):
            row = self.file_row(str(Path(*parts[:i])) if i else '')
            expect(row).to_be_visible()
            if row.locator('.icon').inner_text().strip() == '▸':
                row.click()

    def file_menu(self, rel, item):
        """Right-click a file or folder and choose an item from its menu."""
        self.expand(str(Path(rel).parent) if Path(rel).parent != Path('.') else '')
        row = self.file_row(rel)
        row.click(button='right')
        self.page.locator('.menu').get_by_role('button', name=item).click()

    # ----- file tabs -----

    def tab(self, rel) -> Locator:
        return self.page.locator(f'.file-tab[title="{css_str(self.path(rel))}"]')

    def open(self, rel):
        """The file's tab, opened from the file tree if it isn't open yet."""
        if not self.tab(rel).count():
            self.file_menu(rel, 'Open')
            self.idle()
            self.no_error()
        self.tab(rel).click()
        expect(self.tab(rel)).to_have_class(re.compile(r'\bactive\b'))

    # ----- the structure panel -----

    def select(self, tag, label=None, nth=0, name=None):
        """Selects an element in the File structure tree, by its tag and its label there
        (exactly), or by `name`: a chapter's label is "Title · name"."""
        self.side('File structure')
        self.page.locator('.tree-tools').get_by_role('button', name='Expand all').click()
        rows = self.page.locator(f'.tree .row.element[data-tag="{tag}"]')
        if label is not None:
            rows = rows.and_(self.page.locator(f'[data-label="{css_str(label)}"]'))
        if name is not None:
            rows = rows.and_(self.page.locator(f'[data-label$="· {css_str(name)}"]'))
        row = rows.nth(nth)
        expect(row).to_be_visible()
        # Class names, not substrings: a row holding a hidden selection is "holds-selected".
        if 'selected' not in (row.get_attribute('class') or '').split():
            row.click()
        expect(row).to_have_class(re.compile(r'(^|\s)selected(\s|$)'))

    def current(self) -> Locator:
        """The selected element's breadcrumb: its data-tag names exactly what's selected,
        even an element the tree doesn't list (a list, a paragraph)."""
        return self.page.locator('.crumbs .crumb.current')

    # ----- the main area -----

    @property
    def body(self) -> Locator:
        return self.page.locator('.viewer-body')

    @property
    def viewer(self) -> Locator:
        return self.page.locator('.viewer')

    def field(self, label, scope=None) -> Locator:
        """A form field by its label."""
        return (scope or self.viewer).locator('label.field', has=self.page.locator('.field-label', has_text=re.compile(f'^{re.escape(label)}$'))) \
            .locator('input, select, textarea').first

    def check(self, label, on=True, scope=None):
        box = (scope or self.viewer).locator('label.check', has=self.page.locator('.field-label', has_text=re.compile(f'^{re.escape(label)}$'))) \
            .locator('input[type=checkbox]')
        box.set_checked(on)

    def applies(self, action, status='Saved', count=1):
        """Runs action (a click) and waits for the backend to finish the apply(s) it starts,
        which must end with `status`. Returns the logged changes."""
        before = len(self.app.changes())
        action()
        end = time.time() + TIMEOUT / 1000
        while len(self.app.changes()) < before + count:
            if time.time() > end:
                raise AssertionError(f'no apply finished within {TIMEOUT // 1000}s')
            self.page.wait_for_timeout(50)
        new = self.app.changes()[before:]
        for c in new:
            assert c['result']['status'] == status, c['result']
        self.idle()
        self.no_error()
        if status == 'Saved':
            self.no_problems()
        return new

    def click_applies(self, locator, **kw):
        return self.applies(locator.click, **kw)

    def apply(self, scope=None):
        """Clicks Apply and waits for the file to validate and be written."""
        return self.click_applies((scope or self.viewer).get_by_role('button', name='Apply', exact=True))

    def no_problems(self):
        expect(self.page.locator('.problems .badge')).to_have_text('0')

    def mode(self, name):
        """"Form" or "Source"."""
        self.page.locator('.mode-switch').get_by_role('button', name=name).click()

    # ----- dialogs -----

    @property
    def modal(self) -> Locator:
        return self.page.locator('.modal')

    def dialog_field(self, label) -> Locator:
        return self.field(label, scope=self.modal)

    # ----- the files on disk -----

    def xml(self, rel):
        """The file as it is on disk, parsed."""
        return etree.parse(str(self.root / rel)).getroot()

    def one(self, rel, xpath):
        found = self.xml(rel).xpath(xpath)
        assert len(found) == 1, f'{xpath} in {rel}: {len(found)} found'
        return found[0]


def css_str(s):
    return s.replace('\\', '\\\\').replace('"', '\\"')
