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
    expect(ui.current()).to_have_attribute('data-tag', 'section')
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-label', re.compile('weir$'))
    ui.tab(PIKE).click()
    expect(ui.current()).to_have_attribute('data-tag', 'lair')


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


def edit_on_disk(ui, rel, old, new):
    path = ui.root / rel
    text = path.read_text(encoding='utf-8')
    assert old in text, old
    path.write_text(text.replace(old, new, 1), encoding='utf-8')


def test_a_change_on_disk_is_read_again(edge):
    """With nothing unapplied, the tab reads the file again by itself; the selection
    stays where it can, and the notice says when it can't."""
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='weir')
    note = ui.viewer.locator('.disk-note')
    edit_on_disk(ui, SUPP, 'Below the weir lives', 'Under the weir lives')
    expect(note).to_have_text(re.compile('^The file changed on disk and was read again.'), timeout=10_000)
    expect(ui.body.locator('.body-block.para').first).to_contain_text('Under the weir lives')
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-label', re.compile('weir$'))
    note.get_by_role('button', name='OK').click()
    expect(note).to_have_count(0)

    # The selected section goes: the notice says so (the form it had open is gone).
    text = (ui.root / SUPP).read_text(encoding='utf-8')
    start = text.index('<section name="weir">')
    end = text.index('</section>', start) + len('</section>')
    (ui.root / SUPP).write_text(text[:start] + text[end:], encoding='utf-8')
    expect(note).to_contain_text("The <section> you had open isn't in the file any more.", timeout=10_000)
    expect(ui.page.locator('.tree .row.element[data-label$="· weir"]')).to_have_count(0)


def test_a_change_on_disk_with_unapplied_edits(edge, app):
    """Unapplied edits aren't thrown away behind your back: the tab says the file changed,
    Apply is refused, and "Read it again" drops the edits for the new text."""
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    row = ui.body.locator('.body-block.para').first
    Prose(ui, row).type(' Typed before the file changed.')
    edit_on_disk(ui, SUPP, 'A miller caught in the race', 'A miller caught in the mill race')
    note = ui.viewer.locator('.disk-note')
    expect(note).to_contain_text('Reading it again throws away 1 unapplied edit', timeout=10_000)
    row.get_by_role('button', name='Apply', exact=True).click()
    err = ui.page.locator('.error-modal')
    expect(err.locator('.message')).to_contain_text('has changed on disk since it was opened')
    err.get_by_role('button', name='OK').click()
    assert 'Typed before the file changed.' not in (ui.root / SUPP).read_text(encoding='utf-8')
    note.get_by_role('button', name='Read it again').click()
    expect(note).to_have_text(re.compile('^The file changed on disk and was read again.'))
    para = ui.body.locator('.body-block.para').first
    expect(para.locator('.chip-box')).to_contain_text('A miller caught in the mill race')
    expect(para.locator('.chip-box')).not_to_contain_text('Typed before the file changed.')
    expect(para.locator('.state-badge')).to_have_count(0)


def tree_tags(ui):
    ui.side('File structure')
    ui.page.locator('.tree-tools').get_by_role('button', name='Expand all').click()
    return ui.page.locator('.tree .row.element').evaluate_all('rs => rs.map(r => r.dataset.tag)')


def test_the_tree_stops_at_sections(edge):
    """The tree lists what has a form of its own, and nothing those forms edit: no
    paragraphs, titles, list items or cells."""
    ui = edge
    ui.open(SUPP)
    tags = tree_tags(ui)
    root = ui.xml(SUPP)
    top = [k.tag for k in root]
    chapter_kids = [k.tag for c in root.findall('chapter') for k in c if k.tag != 'chapterTitle']
    assert tags == ['supplement'] + [t for c in root for t in [c.tag] + ([k.tag for k in c if k.tag != 'chapterTitle'] if c.tag == 'chapter' else [])], tags
    assert set(tags) == {'supplement'} | set(top) | set(chapter_kids)
    for hidden in ['p', 'title', 'chapterTitle', 'li', 'tr', 'td', 'headerTitle', 'ch', 'image', 'noun']:
        assert hidden not in tags, hidden

    # A list opened from its section: the section holds the selection; the crumb names it.
    ui.select('section', name='three-pieces')
    ui.body.locator('.body-block', has=ui.page.locator('.ro-tag', has_text='List')).get_by_role('button', name='Open').click()
    expect(ui.current()).to_have_attribute('data-tag', 'list')
    expect(ui.page.locator('.tree .row.holds-selected')).to_have_attribute('data-label', re.compile('three-pieces$'))
    expect(ui.page.locator('.tree .row.selected')).to_have_count(0)

    # A monster file: its stat blocks and family parts.
    ui.open(PIKE)
    tags = tree_tags(ui)
    pike = ui.xml(PIKE)
    assert tags == ['monsters'] + [k.tag for k in pike if k.tag not in ('name', 'shortNoun', 'brief', 'habitats')], tags

    # An item file: one page edits it all.
    ui.open('grand-forks/ferrymans-lantern.xml')
    assert tree_tags(ui) == ['magicItem']
