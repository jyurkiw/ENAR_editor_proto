"""Step 09: the item and spell files. The Ferryman's Lantern (rarities, attunement, a
description paragraph with a data ref, a small table, a reference data entry) and Fork the
Current (school, classes, casting time, range, components with a material, duration,
description, At Higher Levels)."""
import re

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

ITEM = 'grand-forks/ferrymans-lantern.xml'
SPELL = 'fork-the-current.xml'


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def section(ui, title):
    return ui.body.locator('details.sb-section', has=ui.page.locator('summary.sb-section-title', has_text=re.compile('^' + re.escape(title))))


def rows(scope, title):
    return scope.locator('.sb-rows', has=scope.page.locator('.sb-rows-title', has_text=re.compile('^' + re.escape(title)))).first


def prose_field(scope, label):
    return scope.locator('.prose-field', has=scope.page.locator('.field-label', has_text=re.compile('^' + re.escape(label) + '$'))).first


def blocks(ui):
    return section(ui, 'Description').locator('details.sb-item')


def test_the_lantern(opened):
    ui = opened
    ui.open(ITEM)
    ui.select('magicItem')
    c = section(ui, 'Item').locator('.table-card').first
    ui.field('Type', scope=c).select_option('wondrous-item')
    rar = rows(c, 'Rarities')
    rar.get_by_role('button', name='+ Add').click()
    ui.field('Rarity', scope=rar.locator('.sb-row').last).select_option('very-rare')
    ui.field('Note', scope=rar.locator('.sb-row').last).fill('when lit on the river')
    c.locator('label.flag', has_text='Requires attunement').locator('input').check()
    att = rows(c, 'Attunement by')
    att.get_by_role('button', name='+ Add').click()
    ui.field('By', scope=att).fill('a ferryman')
    ui.apply(c)
    r = ui.xml(ITEM)
    assert [(x.text, x.get('note')) for x in r.findall('rarities/rarity')] == [('rare', None), ('very-rare', 'when lit on the river')]
    assert r.find('attunement').get('required') == 'true'
    assert ''.join(r.find('attunement').itertext()).strip() == 'a ferryman', etree.tostring(r.find('attunement'))


def test_a_data_entry(ui):
    sec = section(ui, 'Reference')
    sec.get_by_role('button', name='+ Add ▾').click()
    ui.body.locator('.add-menu button', has=ui.page.locator('.ref-kind', has_text='data')).click()
    card = ui.body.locator('[id^="ref-card-new:"]').last
    expect(card.locator('input[data-attr=id]')).to_be_focused()
    card.locator('input[data-attr=id]').fill('burnHours')
    card.locator('input[data-attr=value]').fill('8')
    ui.apply(card)
    assert dict(ui.one(ITEM, '//reference/data').attrib) == {'id': 'burnHours', 'value': '8'}


def test_the_description(ui):
    para = blocks(ui).first.locator('.sb-item-body')
    p = Prose(ui, prose_field(para, 'Text'))
    p.type('While lit, the lantern shows the safe line across any river. It burns for some hours on one filling of oil.')
    ui.apply(para)
    p.select('some')
    p.insert('8', '8', 'data entry')
    ui.apply(para)
    assert xml_of(ui.one(ITEM, '//description/p')) == \
        'While lit, the lantern shows the safe line across any river. It burns for <dataRef ref="burnHours" value="value"/> hours on one filling of oil.'

    # A table after it: what the flame shows, on a d4.
    ui.click_applies(section(ui, 'Description').get_by_role('button', name='+ Table'))
    table = blocks(ui).last.locator('.sb-item-body')
    grid = table.locator('.item-grid')
    while grid.locator('tbody tr').count() < 4:
        table.get_by_role('button', name='+ Row').click()
    heads = ['d4', 'The flame shows']
    cells = [['1', 'A sandbar'], ['2', 'A snag'], ['3', 'A forkjaw'], ['4', 'Nothing at all']]
    for c, text in enumerate(heads):
        Prose(ui, grid.locator('thead th').nth(c)).replace(text)
    for r, row in enumerate(cells):
        for c, text in enumerate(row):
            Prose(ui, grid.locator('tbody tr').nth(r).locator('td').nth(c)).replace(text)
    ui.apply(table)
    t = ui.one(ITEM, '//description/table')
    assert [''.join(x.itertext()) for x in t.find('header')] == heads, etree.tostring(t)
    assert [[''.join(td.itertext()) for td in tr] for tr in t.find('tableBody')] == cells


def test_the_spell(ui):
    ui.open(SPELL)
    ui.select('spell')
    c = section(ui, 'Spell').locator('.table-card').first
    ui.field('School', scope=c).select_option('Transmutation')
    classes = rows(c, 'Classes')
    classes.get_by_role('button', name='+ Add').click()
    ui.field('Class', scope=classes.locator('.sb-row').last).fill('Druid')
    ui.apply(c)
    assert [x.text for x in ui.xml(SPELL).findall('classes/class')] == ['Wizard', 'Druid']

    cast = section(ui, 'Casting')
    cards = cast.locator('.table-card')
    # Range: 120 feet.
    rng = cards.filter(has=ui.page.locator('.field-label', has_text=re.compile('^Range$')))
    ui.field('Distance', scope=rng).fill('120')
    ui.apply(rng)
    # Components: a material that's consumed.
    comp = cards.filter(has=ui.page.locator('.sb-rows-title', has_text='Materials'))
    mats = rows(comp, 'Materials')
    mats.get_by_role('button', name='+ Add').click()
    Prose(ui, prose_field(mats, 'Material')).type('a forked twig')
    ui.apply(comp)
    # Duration: concentration, up to 1 minute.
    dur = cards.filter(has=ui.page.locator('label.flag', has_text='Concentration'))
    dur.locator('label.flag', has_text='Concentration').locator('input').check()
    ui.field('Number', scope=dur).fill('1')
    ui.field('Unit', scope=dur).select_option('up-to-minute')
    ui.apply(dur)
    s = ui.xml(SPELL)
    assert s.findtext('range/distance') == '120'
    assert 'a forked twig' in etree.tostring(s.find('components/materials'), encoding='unicode')
    assert (s.findtext('duration/concentration'), s.findtext('duration/value'), s.findtext('duration/unit')) == ('true', '1', 'up-to-minute')

    para = section(ui, 'Description').locator('details.sb-item').first.locator('.sb-item-body')
    Prose(ui, prose_field(para, 'Text')).type('You split a current of water into two, each half as strong, for the duration.')
    ui.apply(para)
    hl = section(ui, 'At Higher Levels').locator('.table-card').first
    Prose(ui, prose_field(hl, 'Using a higher-level spell slot')).type('You can split the current one more time for each slot level above 2.')
    ui.apply(hl)
    s = ui.xml(SPELL)
    assert s.findtext('description/p').startswith('You split a current of water')
    assert 'one more time for each slot level' in etree.tostring(s.find('higherLevels'), encoding='unicode')
