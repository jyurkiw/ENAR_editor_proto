"""Step 07: the Forkjaw pike's family, in forkjaw-pike.xml. The header (name, short noun,
subtitle, habitats), a quote box and an image together, a description, a distinction
table, and a lair with three effects (added first and below, moved, one deleted). Then
part ids and moving the parts around."""
import re

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

PIKE = 'grand-forks/forkjaw-pike.xml'


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def root(ui):
    return ui.xml(PIKE)


def part_tags(ui):
    """The family's parts, in file order: everything between the header and the stat blocks."""
    return [k.tag for k in root(ui) if k.tag in ('quoteBox', 'image', 'description', 'distinctionTable', 'lair')]


def family(ui):
    ui.select('monsters')
    expect(ui.body.locator('.fam-header')).to_be_visible()


def part_rows(ui):
    return ui.body.locator('.stat-row', has=ui.page.locator('.part-kind'))


def add_part(ui, kind):
    add = ui.body.locator('.add-part').last
    add.locator('select').select_option(kind)
    ui.click_applies(add.get_by_role('button', name='+ Add', exact=True))


def open_part(ui, n):
    family(ui)
    part_rows(ui).nth(n).locator('.part-summary').click()
    expect(ui.body.locator('.part-bar')).to_be_visible()


def paras(ui):
    return ui.body.locator('.body-block.para')


def test_the_header(opened):
    ui = opened
    ui.open(PIKE)
    family(ui)
    head = ui.body.locator('.fam-header')
    expect(ui.field('Family name', scope=head)).to_have_value('Forkjaw Pike')
    expect(ui.field('Short noun', scope=head)).to_have_value('pike')
    ui.field('Subtitle', scope=head).fill('The river’s oldest grudge')
    habitats = head.locator('.field', has=ui.page.locator('.field-label', has_text=re.compile('^Habitats$')))
    for h in ['River', 'Lake', 'Swamp']:
        habitats.locator('.chip-input').fill(h)
        habitats.get_by_role('button', name='+ Add').click()
    habitats.locator('.chip', has_text='Swamp').get_by_title('Remove').click()
    expect(habitats.locator('.chip')).to_have_count(2)
    treasure = head.locator('.field', has=ui.page.locator('.field-label', has_text=re.compile('^Treasure$')))
    treasure.locator('.chip-input').fill('None')
    treasure.locator('.chip-input').press('Enter')       # Enter adds the name; it doesn't apply
    expect(treasure.locator('.chip')).to_have_text(['None✕'])
    ui.apply(head)
    r = root(ui)
    assert r.findtext('brief') == 'The river’s oldest grudge'
    hab = r.find('habitats')
    assert [h.get('name') for h in hab.findall('habitat')] == ['River', 'Lake'], etree.tostring(hab)
    assert [t.get('name') for t in hab.findall('treasure')] == ['None'], etree.tostring(hab)


def test_quote_box_and_image(ui):
    family(ui)
    expect(ui.body).to_contain_text('No lore or lairs yet.')
    ui.click_applies(ui.body.get_by_role('button', name='+ Quote box & image'))
    assert part_tags(ui) == ['quoteBox', 'image']

    open_part(ui, 0)
    row = paras(ui).first
    Prose(ui, row).type('Never trust a fish with two jaws. It has twice the reasons to bite.')
    ui.apply(row)
    attribution = ui.field('Attribution', scope=ui.body)
    attribution.fill('A Millward fisher')
    ui.applies(lambda: attribution.press('Enter'))
    q = root(ui).find('quoteBox')
    assert q.findtext('quote/p') == 'Never trust a fish with two jaws. It has twice the reasons to bite.'
    assert q.find('attribution').get('name') == 'A Millward fisher'

    open_part(ui, 1)
    card = ui.body.locator('.table-card').nth(1)
    expect(card.locator('.hint.bad')).to_have_text("No file yet: the compiler stops on an image it can't find.")
    ui.field('File', scope=card).fill('art/forkjaw-pike.png')
    ui.field('Alt text', scope=card).fill('A long grey pike with a forked lower jaw')
    ui.field('Title', scope=card).fill('Forkjaw Pike')
    ui.field('Artist', scope=card).fill('Slop Art Studio')
    # The cutline box sits inside a <label>, so a click on it focuses the label's ＋ button
    # instead (see the plan's findings): focus it from script.
    Prose(ui, card.locator('label.field', has_text='Cutline')).type('A forkjaw, drawn from memory by someone who got away.', click=False)
    ui.apply(card)
    img = root(ui).find('image')
    assert {k: img.get(k) for k in ('src', 'alt', 'name', 'credit')} == {
        'src': 'art/forkjaw-pike.png', 'alt': 'A long grey pike with a forked lower jaw', 'name': 'Forkjaw Pike', 'credit': 'Slop Art Studio'}
    assert img.findtext('cutline') == 'A forkjaw, drawn from memory by someone who got away.'


def test_description(ui):
    family(ui)
    add_part(ui, 'description')
    assert part_tags(ui) == ['quoteBox', 'image', 'description']
    open_part(ui, 2)
    texts = ['Forkjaw pike grow for as long as they live, and some of them have lived a long time.',
             'They hunt where the current is fastest, below weirs and mill races.',
             'A paragraph that goes.']
    for i, text in enumerate(texts):
        if i:
            ui.click_applies(paras(ui).nth(i - 1).get_by_role('button', name='+ ¶ below'))
        Prose(ui, paras(ui).nth(i)).type(text)
        ui.apply(paras(ui).nth(i))
    # The second paragraph down past the third, then the third (now second) deleted.
    ui.click_applies(paras(ui).nth(1).get_by_title('Move down'))
    assert [p.text for p in root(ui).findall('description/p')] == [texts[0], texts[2], texts[1]]
    ui.click_applies(paras(ui).nth(1).get_by_title(re.compile('^Delete')))
    assert [p.text for p in root(ui).findall('description/p')] == texts[:2]


def test_distinction_table(ui):
    family(ui)
    add_part(ui, 'distinctionTable')
    open_part(ui, 3)
    grid = ui.body.locator('.dist-grid')
    card = ui.body.locator('.table-card', has=ui.page.locator('.dist-grid'))
    heads = grid.locator('thead input')
    rows = grid.locator('tbody tr')
    want_heads = ['1d4', 'Distinction']
    want_rows = [['1', 'A hook still in its lip'], ['2', 'One eye, milky white'], ['3', 'A ring of rust-coloured scales'],
                 ['4', 'Unusually polite']]
    while heads.count() < 2:
        card.get_by_role('button', name='+ Column').click()
    while heads.count() > 2:
        heads.last.locator('xpath=..').get_by_title('Delete this column').click()
    while rows.count() < len(want_rows) + 1:
        card.get_by_role('button', name='+ Row').click()
    for c, h in enumerate(want_heads):
        heads.nth(c).fill(h)
    # One row too many, filled in the wrong order: fix it in the draft.
    order = [want_rows[0], want_rows[2], want_rows[1], want_rows[3], ['5', 'A stray row']]
    # A cell redraws on every keystroke and its caret goes back to the start, so typed text
    # comes out backwards (see the plan's findings): each cell's text goes in in one event.
    for r, cells in enumerate(order):
        for c, text in enumerate(cells):
            box = rows.nth(r).locator('td').nth(c)
            Prose(ui, box).replace(text)
            expect(box.locator('.chip-box')).to_have_text(text)
    rows.nth(2).get_by_title('Move up').click()
    rows.nth(4).get_by_title('Delete this row').click()
    expect(rows).to_have_count(4)
    ui.apply(card)
    t = root(ui).find('distinctionTable')
    assert [th.text for th in t.iter('dh')] == want_heads, etree.tostring(t)
    assert [[''.join(td.itertext()) for td in tr.findall('td')] for tr in t.iter('tr') if tr.findall('td')] == want_rows


def test_lair(ui):
    family(ui)
    add_part(ui, 'lair')
    open_part(ui, 4)
    card = ui.body.locator('.table-card').nth(1)
    ui.field('Heading', scope=card).fill('The Matriarch’s Pool')
    ui.field('Effects end when it…', scope=card).fill('dies')
    ui.apply(card)
    lair = root(ui).find('lair')
    assert lair.get('name') == 'The Matriarch’s Pool'
    assert lair.findtext('effectEndVerb') == 'dies'

    ui.click_applies(ui.body.get_by_role('button', name='+ Add a description'))
    Prose(ui, paras(ui).first).type('The oldest forkjaw keeps a deep pool below the weir.')
    ui.apply(paras(ui).first)

    effects = ui.body.locator('.effect-row')
    want = [('silt', 'Silt Cloud', 'The water turns brown; everything in it is lightly obscured.'),
            ('undertow', 'Undertow', 'Swimmers are pulled 10 feet toward the pool’s centre.'),
            ('stray', 'Stray', 'Goes.')]
    # The first one first, the rest below it.
    for i, (eid, name, text) in enumerate(want):
        if i == 0:
            ui.click_applies(ui.body.get_by_role('button', name='+ effect first'))
        else:
            ui.click_applies(effects.nth(i - 1).get_by_role('button', name='+ effect below'))
        row = effects.nth(i)
        ui.field('Effect name', scope=row).fill(name)
        ui.field('Id', scope=row).fill(eid)
        Prose(ui, row.locator('.chip-editor')).type(text)
        ui.apply(row)
    # Drop the stray and any the new lair came with, then swap the two that are left.
    while effects.count() > 2:
        ui.click_applies(effects.last.get_by_title(re.compile('^Delete')))
    ui.click_applies(effects.nth(0).get_by_title('Move down'))
    got = [(e.get('id'), e.get('name'), ''.join(e.itertext()).strip()) for e in root(ui).find('lair').iter('effect')]
    assert got == [want[1], want[0]], got


def test_part_ids_and_moves(ui):
    open_part(ui, 2)
    bar = ui.body.locator('.part-bar')
    pid = ui.field('Id', scope=bar)
    pid.fill('pike-lore')
    ui.applies(lambda: pid.press('Enter'))          # no Apply button here: Enter, or leaving it
    assert root(ui).find('description').get('id') == 'pike-lore'
    # Up from the description page, then the lair to the top from the family's list.
    ui.click_applies(bar.get_by_title('Move up'))
    assert part_tags(ui) == ['quoteBox', 'description', 'image', 'distinctionTable', 'lair']
    family(ui)
    for i in range(4, 0, -1):
        ui.click_applies(part_rows(ui).nth(i).get_by_title('Move up'))
    assert part_tags(ui) == ['lair', 'quoteBox', 'description', 'image', 'distinctionTable']
    ui.click_applies(part_rows(ui).nth(0).get_by_title('Move down'))
    ui.click_applies(part_rows(ui).nth(1).get_by_title('Move down'))
    ui.click_applies(part_rows(ui).nth(2).get_by_title('Move down'))
    ui.click_applies(part_rows(ui).nth(3).get_by_title('Move down'))
    assert part_tags(ui) == ['quoteBox', 'description', 'image', 'distinctionTable', 'lair']


def test_delete_a_part(ui, app):
    family(ui)
    add_part(ui, 'description')
    assert part_tags(ui).count('description') == 2
    family(ui)
    app.confirms_asked()
    app.answer_confirms(False)
    part_rows(ui).last.get_by_title('Delete').click()
    asked = app.confirms_asked()
    assert len(asked) == 1 and asked[0].startswith('Delete this description?'), asked
    assert part_tags(ui).count('description') == 2
    ui.click_applies(part_rows(ui).last.get_by_title('Delete'))
    app.confirms_asked()
    assert part_tags(ui) == ['quoteBox', 'description', 'image', 'distinctionTable', 'lair']
