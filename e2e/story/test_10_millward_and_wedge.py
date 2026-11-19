"""Step 10: Millward and The Wedge. Ctrl+K library picks that add their <reference>
entries (the pike, the lantern, the spell); an ability check that leads into a skill check
table, a hazard and a saving throw; the pike's monster entry, first "print only…" and then
"print everything, except…"; a magic item block, a spell block, the toll bridge picture
again in a figure (an image used twice), and a page end."""
import re

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def sec(ui, name):
    return ui.one(SUPP, f'//section[@name="{name}"]')


def para(ui, n=0):
    return ui.body.locator('.body-block.para').nth(n)


def kids(ui, name):
    return [k.tag for k in sec(ui, name)]


def add_below(ui, row, tag):
    ui.applies(lambda: row.locator('select.add-menu').select_option(tag))
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', tag)


def back(ui):
    ui.body.get_by_role('button', name='Back to the section').click()
    expect(ui.body.locator('.block-kind-name')).to_have_text('Section')


def library_pick(ui, section, text, words, query, label, sub):
    """Types a paragraph, applies it, then makes `words` a ref to a library entry that
    isn't in <reference> yet: the pick adds the entry too."""
    ui.select('section', name=section)
    row = para(ui)
    p = Prose(ui, row)
    p.type(text)
    ui.apply(row)
    p.select(words)
    ui.applies(lambda: p.insert(query, label, sub))    # adds the entry
    offer = p.editor.locator('.pill-offer')
    if offer.count():
        offer.get_by_role('button', name='No thanks').click()
    ui.apply(row)
    return row


def test_the_weir(opened):
    ui = opened
    ui.open(SUPP)
    row = library_pick(ui, 'weir', 'Below the weir lives a Forkjaw Pike, and the stones are slick.',
                       'Forkjaw Pike', 'forkjaw', 'Forkjaw Pike', 'project monster')
    m = ui.one(SUPP, '//reference/monster[@id="forkjaw-pike"]')
    assert (m.get('file'), m.get('monster')) == ('forkjaw-pike.xml', 'forkjaw-pike'), dict(m.attrib)
    assert '<monsterRef ref="forkjaw-pike" value="name"/>' in xml_of(sec(ui, 'weir').find('p'))

    # Crossing: an ability check that leads into the table, and a fall.
    p = Prose(ui, row)
    p.type(' Crossing on foot takes a check; a slip means a fall.')
    ui.apply(row)
    p.select('a check')
    p.new_tag('ability', 'Ability check', fields={'stat': 'dex', 'skill': 'acrobatics', 'dc': '13', 'table': True})
    ui.apply(row)
    p.select('a fall')
    p.new_tag('hazard', 'Hazard', fields={'name': 'falling'}, text='a fall')
    ui.apply(row)
    p_xml = xml_of(sec(ui, 'weir').find('p'))
    assert '<skillcheck stat="dex" skill="acrobatics" dc="13" table="true"/>' in p_xml, p_xml
    assert '<hazard name="falling">a fall</hazard>' in p_xml, p_xml

    add_below(ui, row, 'skillcheckResultTable')
    body = ui.body
    ui.field('Ability', scope=body).select_option('dex')
    ui.field('Skill', scope=body).select_option('acrobatics')
    results = body.locator('.sb-rows', has=ui.page.locator('.sb-rows-title', has_text=re.compile('^Results'))).first
    results.locator(':scope > button', has_text='+ Add').click()
    res = results.locator(':scope > .sb-row')
    expect(res).to_have_count(2)
    for i, (dc, text) in enumerate([('10', 'You keep your feet, barely.'), ('15', 'You cross without getting wet.')]):
        ui.field('DC', scope=res.nth(i)).fill(dc)
        Prose(ui, res.nth(i).locator('.chip-editor').first).type(text)
    ui.apply(body)
    t = sec(ui, 'weir').find('skillcheckResultTable')
    assert (t.get('stat'), t.get('skill')) == ('dex', 'acrobatics')
    assert [(r.get('dc'), r.findtext('rd')) for r in t.findall('result')] == [
        ('10', 'You keep your feet, barely.'), ('15', 'You cross without getting wet.')]
    back(ui)


def test_the_mills(ui):
    ui.select('section', name='mills')
    row = para(ui)
    p = Prose(ui, row)
    p.type('A miller caught in the race makes a save or is pulled under the wheel.')
    ui.apply(row)
    p.select('a save')
    p.new_tag('saving', 'Saving throw', fields={'stat': 'str', 'dc': '12'})
    ui.apply(row)
    assert '<savingthrow stat="str" dc="12"/>' in xml_of(sec(ui, 'mills').find('p'))


def test_the_pike_entry(ui):
    ui.select('section', name='pike-attack')
    row = para(ui)
    Prose(ui, row).type('The pike attacks anyone who swims out to the matriarch’s pool.')
    ui.apply(row)
    add_below(ui, row, 'monsterEntry')
    body = ui.body
    expect(body).to_contain_text('Forkjaw Pike: 1 family part(s), 1 lair(s), 2 stat block(s) with ids.')

    # Print only…: it starts with everything listed (the family's part, both stat blocks).
    # Drop the matriarch and print the lair with the pike.
    body.locator('label.flag', has_text='Print only…').locator('input').check()
    lst = body.locator('.sb-rows', has=ui.page.locator('.sb-rows-title', has_text='Prints, in this order')).first
    entries = lst.locator(':scope > .sb-row')
    expect(entries).to_have_count(3)
    entries.nth(2).locator(':scope > .sb-row-tools').get_by_title('Remove').click()
    expect(entries).to_have_count(2)
    entries.nth(1).get_by_role('button', name='+ Part').click()
    ui.apply(body)
    e = sec(ui, 'pike-attack').find('monsterEntry')
    assert e.get('ref') == 'forkjaw-pike'
    assert [(k.tag, k.get('ref')) for k in e] == [('part', 'pike-lore'), ('statblock', 'forkjaw-pike')], etree.tostring(e)
    lair = ui.xml('grand-forks/forkjaw-pike.xml').find('lair').get('id')
    assert [p.get('ref') for p in e.find('statblock').findall('part')] == [lair], etree.tostring(e)

    # Everything except the quotes and the description.
    body.locator('label.flag', has_text='Print everything, except…').locator('input').check()
    body.locator('label.flag', has_text='Quotes').locator('input').check()
    omit = body.locator('.sb-rows', has=ui.page.locator('.sb-rows-title', has_text='Leave out these parts'))
    omit.locator('select.add-menu').select_option('pike-lore')
    ui.apply(body)
    e = sec(ui, 'pike-attack').find('monsterEntry')
    assert dict(e.attrib) == {'ref': 'forkjaw-pike', 'omit': 'quoteBox', 'omitParts': 'pike-lore'}
    assert e.find('statblock') is None
    back(ui)


def test_the_market(ui):
    row = library_pick(ui, 'market', 'Ferry hands sell a Ferryman’s Lantern to anyone crossing after dark.',
                       'Ferryman’s Lantern', 'lantern', "Ferryman's Lantern", 'project item')
    assert ui.one(SUPP, '//reference/item').get('file') == 'ferrymans-lantern.xml'
    add_below(ui, row, 'magicItemBlock')
    assert sec(ui, 'market').find('magicItemBlock').get('ref') == ui.one(SUPP, '//reference/item').get('id')
    back(ui)
    # The lantern's picture: the one image with no artist, for the datasheet's check list.
    add_below(ui, ui.body.locator('.body-block').last, 'image')
    ui.field('Image', scope=ui.body).select_option('img-lantern')
    ui.apply(ui.body)
    back(ui)

    # The toll bridge picture again, in a figure: it's credited once.
    add_below(ui, ui.body.locator('.body-block').last, 'figure')
    ui.field('Id', scope=ui.body).fill('fig-bridge-again')
    ui.field('Caption', scope=ui.body).fill('The bridge from the market')
    ui.field('Image', scope=ui.body).select_option('img-toll-bridge')
    ui.apply(ui.body)
    assert sec(ui, 'market').find('figure/image').get('id') == 'img-toll-bridge'
    back(ui)
    add_below(ui, ui.body.locator('.body-block').last, 'pageEnd')
    ui.body.locator('label.flag', has_text='Balance the columns').locator('input').uncheck()
    ui.apply(ui.body)
    assert kids(ui, 'market') == ['title', 'p', 'magicItemBlock', 'image', 'figure', 'pageEnd']
    assert sec(ui, 'market').find('pageEnd').get('balance') == 'false'
    back(ui)


def test_the_ferry_shrine(ui):
    row = library_pick(ui, 'ferry-shrine', 'The shrine’s keeper teaches Fork the Current to ferry hands.',
                       'Fork the Current', 'fork the', 'Fork the Current', 'project spell')
    add_below(ui, row, 'spellBlock')
    assert sec(ui, 'ferry-shrine').find('spellBlock').get('ref') == ui.one(SUPP, '//reference/spell').get('id')
    back(ui)
