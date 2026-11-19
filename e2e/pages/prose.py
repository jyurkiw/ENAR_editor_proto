"""The prose editor (a contenteditable box with chips): typing, selecting a run of text,
right-clicking a word, and inserting pills through Ctrl+K. Fill doesn't work on these, so
everything goes through the keyboard and mouse."""
import re

from playwright.sync_api import Locator, expect

# Finds the nth occurrence of `text` in the box's text nodes (chips skipped) and returns a
# Range over it.
_FIND = """
function find(box, text, nth) {
    const walker = document.createTreeWalker(box, NodeFilter.SHOW_TEXT, {
        acceptNode: n => n.parentElement.closest('[data-xml]') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
    });
    let seen = 0;
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        let i = n.data.indexOf(text);
        while (i >= 0) {
            if (seen++ === nth) {
                const r = document.createRange();
                r.setStart(n, i);
                r.setEnd(n, i + text.length);
                return r;
            }
            i = n.data.indexOf(text, i + 1);
        }
    }
    throw new Error('not in the box: ' + text);
}
"""


class Prose:
    def __init__(self, ui, box: Locator):
        self.ui = ui
        self.page = ui.page
        self.box = box.locator('.chip-box') if 'chip-box' not in (box.get_attribute('class') or '') else box
        self.editor = self.box.locator('xpath=ancestor::div[contains(@class,"chip-editor")][1]')

    def focus_end(self, click=True):
        """Puts the caret at the end of the box: by clicking it, or (click=False, for a box
        a click can't reach) by focusing it from script."""
        if click:
            self.box.click()
            self.page.keyboard.press('Control+End')
        else:
            self.box.evaluate("""box => {
                box.focus();
                const r = document.createRange();
                r.selectNodeContents(box);
                r.collapse(false);
                const s = getSelection();
                s.removeAllRanges();
                s.addRange(r);
            }""")

    def type(self, text, click=True):
        """Types at the end of the box."""
        self.focus_end(click)
        self.page.keyboard.type(text)

    def replace(self, text):
        """Replaces the box's text in one input event (select all, then insert), for boxes
        that redraw on every keystroke."""
        self.box.click()
        self.page.keyboard.press('Control+a')
        self.page.keyboard.insert_text(text)

    def text(self):
        return self.box.inner_text()

    def chips(self) -> Locator:
        return self.box.locator('.chip-tag')

    def _rect(self, text, nth, select):
        return self.box.evaluate(
            '(box, [text, nth, select]) => {' + _FIND + """
                const r = find(box, text, nth);
                if (select) {
                    box.focus();
                    const s = getSelection();
                    s.removeAllRanges();
                    s.addRange(r);
                }
                const b = r.getClientRects()[0];
                return {x: b.left + b.width / 2, y: b.top + b.height / 2};
            }""",
            [text, nth, select],
        )

    def select(self, text, nth=0):
        """Selects the nth occurrence of text."""
        return self._rect(text, nth, True)

    def caret_after(self, text, nth=0):
        """Puts the caret just after the nth occurrence of text."""
        self.box.evaluate('(box, [text, nth]) => {' + _FIND + """
            const r = find(box, text, nth);
            r.collapse(false);
            box.focus();
            const s = getSelection();
            s.removeAllRanges();
            s.addRange(r);
        }""", [text, nth])

    def menu(self, text, nth=0, select=False) -> Locator:
        """Right-clicks the nth occurrence of text (selecting it first, if asked) and
        returns the menu that opens."""
        p = self._rect(text, nth, select)
        self.page.mouse.click(p['x'], p['y'], button='right')
        m = self.editor.locator('.prose-menu')
        expect(m).to_be_visible()
        return m

    def menu_item(self, menu: Locator, label) -> Locator:
        return menu.get_by_role('button', name=re.compile('^' + re.escape(label)))

    def generator(self, query=None) -> Locator:
        """Opens the ref generator (Ctrl+K) at the caret, typing `query` into its search
        (None keeps what the selection put there)."""
        self.page.keyboard.press('Control+k')
        panel = self.editor.locator('.gen-panel')
        search = panel.locator('.gen-search')
        expect(search).to_be_focused()
        if query is not None:
            search.fill(query)
        return panel

    def gen_item(self, panel: Locator, label, sub=None) -> Locator:
        item = panel.locator('.gen-item').filter(has=self.page.locator('.gen-label', has_text=re.compile('^' + re.escape(label) + '$')))
        if sub is not None:
            item = item.filter(has=self.page.locator('.gen-sub', has_text=sub))
        return item.first

    def insert(self, query, label, sub=None):
        """Ctrl+K, search, and pick a ref that inserts at once."""
        panel = self.generator(query)
        self.gen_item(panel, label, sub).click()
        expect(panel).to_have_count(0)

    def new_tag(self, query, label, fields=None, text=None):
        """Ctrl+K, pick one of the other tags, fill its fields and Insert."""
        panel = self.generator(query)
        self.gen_item(panel, label).click()
        form = self.editor.locator('.insert-panel')
        expect(form.locator('.insert-title')).to_have_text(label)
        if text is not None:
            ctl = self.ui.field('text *', scope=form)
            if ctl.evaluate('e => e.tagName') == 'SELECT':
                ctl.select_option(text)
            else:
                ctl.fill(text)
        for name, value in (fields or {}).items():
            ctl = form.locator('label.field', has=self.page.locator('.field-label', has_text=re.compile(f'^{re.escape(name)}( \\*)?$'))) \
                .locator('input, select').first
            kind = ctl.evaluate('e => e.tagName + ":" + e.type')
            if kind.startswith('SELECT'):
                ctl.select_option(value)
            elif kind.endswith('checkbox'):
                ctl.set_checked(bool(value))
            else:
                ctl.fill(value)
        form.get_by_role('button', name='Insert').click()
        expect(form).to_have_count(0)
