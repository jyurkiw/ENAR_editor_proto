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
    row = ui.body.locator('.row-card').filter(has=ui.page.locator('.row-name', has_text='Row 4'))
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


def test_closing_a_tab_with_unapplied_edits(edge, app):
    """It asks (issue 15): Cancel keeps the tab and its edits; OK closes it, and the edits
    go. Clicking × doesn't take the focus out of a paragraph, so nothing is applied."""
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    before = (ui.root / SUPP).read_bytes()
    row = ui.body.locator('.body-block.para').first
    Prose(ui, row).type(' Words typed before closing.')
    n = len(app.changes())
    app.confirms_asked()
    app.answer_confirms(False)
    ui.tab(SUPP).locator('button.close').click()
    assert app.confirms_asked() == ['Close supplement.xml? Its 1 unapplied edit will be lost.']
    expect(ui.tab(SUPP)).to_have_count(1)
    expect(row.locator('.state-badge')).to_have_text('unapplied')
    ui.page.wait_for_timeout(300)
    assert len(app.changes()) == n and (ui.root / SUPP).read_bytes() == before
    app.answer_confirms(True)
    ui.tab(SUPP).locator('button.close').click()
    assert len(app.confirms_asked()) == 1
    expect(ui.tab(SUPP)).to_have_count(0)
    ui.page.wait_for_timeout(300)
    assert len(app.changes()) == n and (ui.root / SUPP).read_bytes() == before

    # A stat block card's draft counts too.
    ui.open(PIKE)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    sec = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Heading')))
    ui.field('Armor Class', scope=sec.locator('.table-card').first).fill('19')
    ui.tab(PIKE).locator('button.close').click()
    assert app.confirms_asked() == ['Close forkjaw-pike.xml? Its 1 unapplied edit will be lost.']
    expect(ui.tab(PIKE)).to_have_count(0)

    # With nothing unapplied, it just closes.
    ui.open(PIKE)
    ui.tab(PIKE).locator('button.close').click()
    expect(ui.tab(PIKE)).to_have_count(0)
    assert app.confirms_asked() == []


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
