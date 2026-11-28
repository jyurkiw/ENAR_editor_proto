"""Step 05: Chapter 1's prose. Paragraphs typed, added below and at the start, dragged and
deleted; the river, the town and the road made nouns ("Turn into a person, place or
thing"), with every mention of the river pilled through Review…; a blue mention made a
pill from the right-click menu; a word added to the project word list; Ctrl+K for a
condition, an ability check, a chapter and a figure; a read-aloud box; a numbered list;
the town map as a figure.

Chips go in one at a time, replacing placeholder words in an applied paragraph; typing
on through a chip is in edges/test_prose.py."""
import re

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'
SALLOW = ('Grand Forks sits where the River Sallow splits around a wedge of high ground. '
          'The Sallow is slow and brown, and the Sallow turns every mill in town. '
          'Swimmers who fight the current end up restrained unless they pass a DC 12 Athletics check; '
          'the chapter on Tollside has the fines.')
TRAVELLERS = ('Travellers on the Copperway reach Grand Forks after three days of empty road, '
              'and the Tollwarden meets them at the bridge.')


def sec(ui, name):
    return ui.one(SUPP, f'//section[@name="{name}"]')


def xml_of(el):
    """An element's content as written, without the namespace lxml adds."""
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def paras(ui):
    return ui.body.locator('.body-block.para')


def blocks(ui):
    return ui.body.locator('.body-block')


def noun_form(p: Prose, short=None, kind='place'):
    form = p.editor.locator('.insert-panel')
    expect(form.locator('.insert-title')).to_have_text('Turn into a person, place or thing')
    if short:
        form.locator('label.pill-field', has_text='Short name').locator('input').fill(short)
    form.locator('label.pill-field', has_text='Kind').locator('select').select_option(kind)
    return form


def no_thanks(p: Prose):
    offer = p.editor.locator('.pill-offer')
    if offer.count():
        offer.get_by_role('button', name='No thanks').click()
        expect(offer).to_have_count(0)


def test_type_the_opening(opened):
    ui = opened
    ui.open(SUPP)
    ui.select('section', label='intro-opening')
    expect(paras(ui)).to_have_count(1)
    row = paras(ui).first
    Prose(ui, row).type(SALLOW)
    expect(row.locator('.state-badge')).to_have_text('unapplied')
    ui.apply(row)
    expect(row.locator('.state-badge')).to_have_count(0)
    assert sec(ui, 'intro-opening').findtext('p') == SALLOW


def test_the_river_is_a_place(ui):
    row = paras(ui).first
    p = Prose(ui, row)
    menu = p.menu('River Sallow', select=True)
    p.menu_item(menu, 'Turn into a person, place or thing').click()
    form = noun_form(p, short='Sallow')
    ui.click_applies(form.get_by_role('button', name='Make it'))
    # The chip names the new entry, even with the box still focused (issue 10).
    expect(p.chips()).to_have_text(['River Sallow'])
    expect(p.chips().first).not_to_have_class(re.compile(r'chip-bad'))
    noun = ui.one(SUPP, '//reference/noun[@id="river-sallow"]')
    assert dict(noun.attrib) == {'id': 'river-sallow', 'name': 'River Sallow', 'shortName': 'Sallow', 'kind': 'place'}

    # Review… applies the paragraph (with its new chip), then lists the other mentions.
    offer = p.editor.locator('.pill-offer')
    expect(offer).to_contain_text('“River Sallow”')
    ui.click_applies(offer.get_by_role('button', name='Review…'))
    dialog = ui.modal
    expect(dialog.locator('h2')).to_have_text('Pill every mention: “River Sallow”')
    expect(dialog).to_contain_text('Applied 1 paragraph you were editing first.')
    rows = dialog.locator('.pill-all-row')
    expect(rows).to_have_count(2)
    expect(rows.locator('mark')).to_have_text(['Sallow', 'Sallow'])
    # Untick one and tick it again: the count follows.
    rows.first.locator('input').uncheck()
    expect(dialog.get_by_role('button', name='Replace 1')).to_be_visible()
    rows.first.locator('input').check()
    ui.click_applies(dialog.get_by_role('button', name='Replace 2'))
    expect(ui.modal).to_have_count(0)

    p_xml = xml_of(sec(ui, 'intro-opening').find('p'))
    assert p_xml.startswith('Grand Forks sits where the <nounRef ref="river-sallow" value="name"/> splits')
    assert p_xml.count('<nounRef ref="river-sallow" value="shortName"/>') == 2
    expect(p.chips()).to_have_text(['River Sallow', 'Sallow', 'Sallow'])


def test_the_town_is_a_place(ui):
    row = paras(ui).first
    p = Prose(ui, row)
    menu = p.menu('Grand Forks', select=True)
    p.menu_item(menu, 'Turn into a person, place or thing').click()
    form = noun_form(p)
    ui.click_applies(form.get_by_role('button', name='Make it'))
    no_thanks(p)
    ui.apply(row)
    assert xml_of(sec(ui, 'intro-opening').find('p')).startswith('<nounRef ref="grand-forks" value="name"/> sits where')
    assert ui.one(SUPP, '//reference/noun[@id="grand-forks"]').get('kind') == 'place'


def test_a_second_paragraph(ui, app):
    ui.click_applies(paras(ui).first.get_by_role('button', name='+ ¶ below'))
    expect(paras(ui)).to_have_count(2)
    row = paras(ui).nth(1)
    expect(row.locator('.chip-box')).to_be_focused()
    p = Prose(ui, row)
    p.type(TRAVELLERS)

    # The town is a noun now: its mention is underlined, and the menu makes it a pill.
    menu = p.menu('Grand Forks')
    expect(menu.locator('.menu-title')).to_have_text('“Grand Forks”')
    p.menu_item(menu, 'Make pill: Grand Forks · name').click()
    expect(p.chips()).to_have_text(['Grand Forks'])

    # A word the dictionary doesn't know: suggestions, then into the project's word list.
    menu = p.menu('Tollwarden')
    expect(menu.locator('.menu-title')).to_have_text('“Tollwarden”')
    expect(p.menu_item(menu, 'Toll warden')).to_be_visible()
    p.menu_item(menu, 'Add to project words').click()
    words = app.project_dir / 'enigma-words.txt'
    for _ in range(100):
        if words.exists():
            break
        ui.page.wait_for_timeout(50)
    assert 'Tollwarden' in words.read_text(encoding='utf-8').splitlines()
    ui.apply(row)

    # The road: a place too.
    menu = p.menu('Copperway', select=True)
    p.menu_item(menu, 'Turn into a person, place or thing').click()
    ui.click_applies(noun_form(p).get_by_role('button', name='Make it'))
    no_thanks(p)
    ui.apply(row)
    got = xml_of(sec(ui, 'intro-opening').findall('p')[1])
    assert got == ('Travellers on the <nounRef ref="copperway" value="name"/> reach <nounRef ref="grand-forks" value="name"/> '
                   'after three days of empty road, and the Tollwarden meets them at the bridge.')


def test_drag_the_travellers_first(ui):
    """Dropped on the top half of the first paragraph: it goes before it."""
    first, second = paras(ui).nth(0), paras(ui).nth(1)
    ui.applies(lambda: second.locator('.grip').drag_to(first, target_position={'x': 40, 'y': 5}))
    texts = [xml_of(p) for p in sec(ui, 'intro-opening').findall('p')]
    assert texts[0].startswith('Travellers on the') and texts[1].startswith('<nounRef ref="grand-forks" value="name"/> sits'), texts
    expect(Prose(ui, paras(ui).first).chips()).to_have_text(['Copperway', 'Grand Forks'])


def test_delete_a_paragraph(ui, app):
    ui.click_applies(paras(ui).last.get_by_role('button', name='+ ¶ below'))
    row = paras(ui).nth(2)
    Prose(ui, row).type('A paragraph that goes.')
    ui.apply(row)
    assert len(sec(ui, 'intro-opening').findall('p')) == 3
    delete = row.get_by_title('Delete paragraph')
    app.confirms_asked()
    app.answer_confirms(False)
    delete.click()
    assert app.confirms_asked() == ['Delete this paragraph?']
    assert len(sec(ui, 'intro-opening').findall('p')) == 3
    ui.click_applies(delete)
    assert app.confirms_asked() == ['Delete this paragraph?']
    expect(paras(ui)).to_have_count(2)
    assert len(sec(ui, 'intro-opening').findall('p')) == 2


def chip_for(ui, row, words, query, label, **kw):
    """Selects `words` in a paragraph with nothing unapplied, makes them a chip, applies."""
    p = Prose(ui, row)
    expect(row.locator('.state-badge')).to_have_count(0)
    p.select(words)
    if kw:
        p.new_tag(query, label, **kw)
    else:
        p.insert(query, label, 'chapter' if label == 'Tollside' else None)
    ui.apply(row)


def test_ctrl_k(ui):
    row = paras(ui).nth(1)
    chip_for(ui, row, 'restrained', 'cond', 'Condition', text='restrained')
    chip_for(ui, row, 'a DC 12 Athletics check', 'ability', 'Ability check', fields={'stat': 'str', 'skill': 'athletics', 'dc': '12'})
    chip_for(ui, row, 'Tollside', None, 'Tollside')
    expect(Prose(ui, row).chips()).to_have_text(
        ['Grand Forks', 'River Sallow', 'Sallow', 'Sallow', 'Restrained', 'DC 12 Strength (Athletics) check', 'Chapter 2: Tollside'])
    got = xml_of(sec(ui, 'intro-opening').findall('p')[1])
    assert got.endswith('Swimmers who fight the current end up <condition>restrained</condition> unless they pass '
                        '<skillcheck stat="str" skill="athletics" dc="12"/>; the chapter on <chapterRef name="tollside"/> has the fines.'), got

    # A chip's own form: change the DC, then save.
    p = Prose(ui, row)
    p.chips().filter(has_text='Athletics').click()
    form = p.editor.locator('.insert-panel')
    expect(form.locator('.insert-title')).to_have_text('Ability check')
    ui.field('dc', scope=form).fill('13')
    form.get_by_role('button', name='Save').click()
    expect(row.locator('.state-badge')).to_have_text('unapplied')      # the focus stays in the box
    ui.apply(row)
    assert '<skillcheck stat="str" skill="athletics" dc="13"/>' in xml_of(sec(ui, 'intro-opening').findall('p')[1])


def test_read_aloud_box(ui):
    # After the first paragraph: the compiler wants a chapter to open with one.
    ui.applies(lambda: paras(ui).first.locator('select.add-menu').select_option('speakAloud'))
    expect(ui.current()).to_have_attribute('data-tag', 'speakAloud')
    # The first box is the trigger (what leads into it), then the box's paragraph.
    rows = ui.body.locator('.body-block')
    expect(rows).to_have_count(2)
    for row, text in [(rows.nth(0), 'When the party first sees the town:'),
                      (rows.nth(1), 'The road crests a low hill, and below you the river splits in two around a town built on three banks.')]:
        Prose(ui, row).type(text)
        ui.apply(row)
    assert [k.tag for k in sec(ui, 'intro-opening')][:3] == ['p', 'speakAloud', 'p']
    box = sec(ui, 'intro-opening').find('speakAloud')
    assert box.findtext('trigger') == 'When the party first sees the town:'
    assert box.findtext('p').startswith('The road crests a low hill')


def test_the_map_figure(ui):
    ui.select('section', label='intro-opening')
    ui.applies(lambda: blocks(ui).last.locator('select.add-menu').select_option('figure'))
    expect(ui.current()).to_have_attribute('data-tag', 'figure')
    ui.field('Id', scope=ui.body).fill('fig-town-map')
    ui.field('Caption', scope=ui.body).fill('Grand Forks and the Sallow')
    ui.field('Image', scope=ui.body).select_option('img-map')
    ui.field('Width', scope=ui.body).select_option('both')
    ui.apply(ui.body)
    fig = sec(ui, 'intro-opening').find('figure')
    assert (fig.get('id'), fig.get('name')) == ('fig-town-map', 'Grand Forks and the Sallow')
    assert dict(fig.find('image').attrib) == {'id': 'img-map', 'span': 'both'}

    # A paragraph after it that points at it.
    ui.body.get_by_role('button', name='Back to the section').click()
    fig_row = blocks(ui).filter(has=ui.page.locator('.ro-tag', has_text='Figure'))
    ui.click_applies(fig_row.get_by_role('button', name='+ ¶ below'))
    row = paras(ui).last
    Prose(ui, row).type('The map shows all three pieces.')
    ui.apply(row)
    chip_fig = Prose(ui, row)
    chip_fig.select('The map')
    chip_fig.insert('fig', 'fig-town-map', 'figure')
    ui.apply(row)
    assert xml_of(sec(ui, 'intro-opening').findall('p')[-1]) == '<figureRef ref="fig-town-map"/> shows all three pieces.'


def test_a_numbered_list(ui):
    ui.select('section', name='three-pieces')
    row = paras(ui).first
    Prose(ui, row).type('The town is really three towns:')
    ui.apply(row)
    # One before it, which gets the focus.
    ui.click_applies(ui.body.locator('.body-tools').get_by_role('button', name='+ ¶ at start'))
    expect(paras(ui)).to_have_count(2)
    expect(paras(ui).first.locator('.chip-box')).to_be_focused()
    Prose(ui, paras(ui).first).type('Each piece still keeps its own customs.')
    ui.apply(paras(ui).first)
    assert [p.text for p in sec(ui, 'three-pieces').findall('p')] == ['Each piece still keeps its own customs.', 'The town is really three towns:']
    row = paras(ui).nth(1)
    ui.applies(lambda: row.locator('select.add-menu').select_option('list'))
    expect(ui.current()).to_have_attribute('data-tag', 'list')
    items = ['Tollside, on the north bank, where the road arrives.', 'Millward, on the east fork.',
             'The Wedge, on the high ground between.']
    for i, text in enumerate(items):
        rows = ui.body.locator('.body-block')
        if i:
            ui.click_applies(rows.nth(i - 1).get_by_role('button', name='+ item below'))
        row = ui.body.locator('.body-block').nth(i)
        Prose(ui, row).type(text)
        ui.apply(row)
    lst = sec(ui, 'three-pieces').find('list')
    assert [li.text for li in lst.findall('li')] == items
    assert lst.get('bullet') in (None, 'bullet')
    ui.applies(lambda: ui.field('Markers', scope=ui.body).select_option('number'))
    expect(ui.body.locator('.li-marker')).to_have_text(['1.', '2.', '3.'])
    assert sec(ui, 'three-pieces').find('list').get('bullet') == 'number'


def test_list_items_move_and_go(ui, app):
    """A fourth item, dragged up above the third and then deleted; a second list, deleted
    whole."""
    rows = ui.body.locator('.body-block')
    ui.click_applies(rows.nth(2).get_by_role('button', name='+ item below'))
    Prose(ui, rows.nth(3)).type('A stray item.')
    ui.apply(rows.nth(3))
    ui.applies(lambda: rows.nth(3).locator('.grip').drag_to(rows.nth(2), target_position={'x': 40, 'y': 4}))
    lis = lambda: [li.text for li in sec(ui, 'three-pieces').find('list').findall('li')]
    assert lis()[2] == 'A stray item.', lis()
    app.confirms_asked()
    ui.click_applies(rows.nth(2).get_by_title('Delete item'))
    assert len(app.confirms_asked()) <= 1          # an item with text asks first
    assert lis() == ['Tollside, on the north bank, where the road arrives.', 'Millward, on the east fork.',
                     'The Wedge, on the high ground between.']

    # A second list after the first, then "Delete list" (Cancel first).
    ui.body.get_by_role('button', name='open it').click()
    lst = ui.body.locator('.body-block', has=ui.page.locator('.ro-tag', has_text='List'))
    ui.applies(lambda: lst.locator('select.add-menu').select_option('list'))
    expect(ui.current()).to_have_attribute('data-tag', 'list')
    assert len(sec(ui, 'three-pieces').findall('list')) == 2
    delete = ui.body.get_by_role('button', name='Delete list')
    app.answer_confirms(False)
    delete.click()
    assert app.confirms_asked() == ['Delete this list and its 1 item(s)?']
    ui.click_applies(delete)
    app.confirms_asked()
    assert len(sec(ui, 'three-pieces').findall('list')) == 1
