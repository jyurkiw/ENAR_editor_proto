"""Files and tabs: opening files the editor can't take (not well-formed, not a file it
knows, failing its schema) shows the error and opens no tab; a row one cell short of its
headers, written by hand, gets "+ cell"; each tab keeps its own selection; the generator's
list keeps its groups in order and caps refs."""
import re

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'
PIKE = 'grand-forks/forkjaw-pike.xml'


def reopen(ui, rel):
    """Closes the file's tab, if it's open, and opens it again: the editor reads the file
    from disk only when it opens it."""
    tab = ui.tab(rel)
    if tab.count():
        tab.locator('button.close').click()
        expect(tab).to_have_count(0)
    ui.file_menu('', 'Refresh')
    ui.open(rel)


BAD = {
    'broken.xml': '<?xml version="1.0"?>\n<supplement type="settlement">\n  <chapter name="x">\n</supplement>\n',
    'not-ours.xml': '<?xml version="1.0"?>\n<shoppingList><item>eggs</item></shoppingList>\n',
    'invalid-monster.xml': ('<?xml version="1.0" encoding="UTF-8"?>\n'
                            '<monsters xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" '
                            'xsi:noNamespaceSchemaLocation="../../../schemas/monster-template-schema.xsd">\n'
                            '    <name>Bad</name>\n    <monster id="bad"><colour>blue</colour></monster>\n</monsters>\n'),
}

# What the error dialog says for each.
WHY = {
    'broken.xml': "The file isn't well-formed XML (line 4)",
    'not-ours.xml': "isn't a supplement, monster, magic item, spell or license file",
    'invalid-monster.xml': "doesn't match the monster file schema",
}


def test_files_it_cannot_open(edge):
    ui = edge
    for name, text in BAD.items():
        (ui.root / name).write_text(text, encoding='utf-8')
    ui.file_menu('', 'Refresh')
    for name, why in WHY.items():
        tabs = ui.page.locator('.file-tab').count()
        ui.file_menu(name, 'Open')
        ui.idle()
        err = ui.page.locator('.error-modal')
        expect(err.locator('.message')).to_contain_text(why)
        err.get_by_role('button', name='OK').click()
        expect(err).to_have_count(0)
        assert ui.page.locator('.file-tab').count() == tabs, name
    for name in BAD:
        (ui.root / name).unlink()
    ui.file_menu('', 'Refresh')


def test_a_short_row_gets_a_cell(edge):
    """The editor never writes a row with fewer cells than headers; a person might."""
    ui = edge
    path = ui.root / SUPP
    text = path.read_text(encoding='utf-8')
    short = '<tr><td>Ferry passengers</td><td>3 cp</td></tr>'
    text = text.replace('</tableBody>', f'    {short}\n            </tableBody>', 1)
    path.write_text(text, encoding='utf-8')
    reopen(ui, SUPP)
    ui.select('table')
    row = ui.body.locator('.row-card').last
    expect(row.locator('.cell-missing')).to_be_visible()
    ui.click_applies(row.get_by_role('button', name='+ cell'))
    tr = etree.parse(str(path)).getroot().findall('.//table/tableBody/tr')[-1]
    assert [''.join(td.itertext()) for td in tr.findall('td')] == ['Ferry passengers', '3 cp', '']


def test_each_tab_keeps_its_selection(edge):
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='weir')
    ui.open(PIKE)
    ui.select('lair')
    ui.tab(SUPP).click()
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'section')
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-label', re.compile('weir$'))
    ui.tab(PIKE).click()
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'lair')


def test_closing_a_tab_with_unapplied_edits(edge):
    """What happens today, without asking (see the plan's findings): a paragraph being
    edited is applied, because closing the tab takes the focus out of it; a stat block
    card's draft, which applies only on Apply or Enter, is thrown away."""
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    row = ui.body.locator('.body-block.para').first
    Prose(ui, row).type(' Words typed before closing.')
    n = len(ui.app.changes())
    ui.tab(SUPP).locator('button.close').click()
    expect(ui.tab(SUPP)).to_have_count(0)
    for _ in range(200):                  # the apply the close set off (no tab left to watch)
        if len(ui.app.changes()) > n:
            break
        ui.page.wait_for_timeout(50)
    assert ui.app.changes()[-1]['result']['status'] == 'Saved'
    assert 'Words typed before closing.' in (ui.root / SUPP).read_text(encoding='utf-8')

    ui.open(PIKE)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    before = (ui.root / PIKE).read_bytes()
    sec = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Heading')))
    card = sec.locator('.table-card').first
    ui.field('Armor Class', scope=card).fill('19')
    expect(card).to_have_class(re.compile(r'\bdirty\b'))
    n = len(ui.app.changes())
    ui.tab(PIKE).locator('button.close').click()
    expect(ui.tab(PIKE)).to_have_count(0)
    ui.page.wait_for_timeout(500)
    assert len(ui.app.changes()) == n
    assert (ui.root / PIKE).read_bytes() == before
    ui.open(PIKE)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    expect(ui.field('Armor Class', scope=card)).not_to_have_value('19')


def test_the_generator_list(edge):
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    p = Prose(ui, ui.body.locator('.body-block.para').first)
    p.focus_end()
    panel = p.generator('')
    expect(panel.locator('.gen-item').first).to_be_visible()
    ui.page.wait_for_timeout(500)                       # the library arrives
    groups = panel.locator('.gen-group').all_inner_texts()
    assert [g.lower() for g in groups] == ['entries', 'chapters & figures', 'library', 'other tags'], groups
    items = panel.locator('.gen-item')
    subs = items.locator('.gen-sub').all_inner_texts()
    tags = sum(1 for s in subs if not re.search(r'entry|chapter|figure|SRD|project', s))
    assert items.count() - tags <= 40, (items.count(), tags)       # refs and library capped at 40
    ui.page.keyboard.press('Escape')
    expect(panel).to_have_count(0)
