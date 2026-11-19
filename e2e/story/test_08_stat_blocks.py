"""Step 08: stat blocks. The Forkjaw Pike filled in card by card (heading and speeds,
ability scores, attributes and CR, traits, attacks with a hit that starts with a comma and a
<dc> in the grapple text, actions moved and one deleted, legendary actions and their
uses); a second stat block, the Forkjaw Matriarch, added after it and given the lair; in
townsfolk.xml stat blocks added, moved and deleted; the Preview panel."""
import re

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

PIKE = 'grand-forks/forkjaw-pike.xml'
TOWN = 'townsfolk.xml'


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def mon(ui, rel, mid):
    return ui.one(rel, f'/monsters/monster[@id="{mid}"]')


def section(ui, title):
    return ui.body.locator('details.sb-section', has=ui.page.locator('summary.sb-section-title', has_text=re.compile('^' + re.escape(title))))


def card(ui, title):
    return section(ui, title).locator('.table-card').first


def rows(scope, title):
    return scope.locator('.sb-rows', has=scope.page.locator('.sb-rows-title', has_text=re.compile('^' + re.escape(title)))).first


def item(ui, list_title, name):
    """A trait's or action's card in a list, by the name on its line."""
    return section(ui, list_title).locator('details.sb-item', has=ui.page.locator('.sb-item-name', has_text=re.compile('^' + re.escape(name) + '$')))


def items(ui, list_title):
    return section(ui, list_title).locator('details.sb-item')


def open_item(it):
    if not it.evaluate('d => d.open'):
        it.locator('summary .sb-item-name').click()
    return it.locator('.sb-item-body')


def prose_field(scope, label):
    return scope.locator('.prose-field', has=scope.page.locator('.field-label', has_text=re.compile('^' + re.escape(label) + '$'))).first


def stat_block(ui, rel, name):
    ui.open(rel)
    ui.side('File structure')
    ui.page.locator('.tree-tools').get_by_role('button', name='Expand all').click()
    row = ui.page.locator('.tree .row.element[data-tag="monster"]').filter(has_text=name).first
    row.click()
    expect(card(ui, 'Stat block')).to_be_visible()
    expect(ui.field('Name', scope=card(ui, 'Stat block'))).to_have_value(name)


def test_heading(opened):
    ui = opened
    stat_block(ui, PIKE, 'Forkjaw Pike')
    c = card(ui, 'Heading')
    ui.field('Size', scope=rows(c, 'Sizes')).select_option('Large')
    ui.field('Type', scope=c).fill('Beast')
    ui.field('Alignment', scope=c).fill('Unaligned')
    ui.field('Armor Class', scope=c).fill('13')
    ui.field('Hit dice', scope=c).fill('5')
    speeds = rows(c, 'Speeds')
    ui.field('Feet', scope=speeds.locator('.sb-row').first).fill('10')
    speeds.get_by_role('button', name='+ Add').click()
    swim = speeds.locator('.sb-row').nth(1)
    ui.field('Kind', scope=swim).select_option('swim')
    ui.field('Feet', scope=swim).fill('40')
    ui.field('Id', scope=swim).fill('swim')
    expect(c).to_have_class(re.compile(r'\bdirty\b'))
    ui.apply(c)
    h = mon(ui, PIKE, 'forkjaw-pike').find('heading')
    assert [h.findtext('sizes/size'), h.findtext('type'), h.findtext('alignment'), h.findtext('armorClass'), h.findtext('hitDice')] == \
        ['Large', 'Beast', 'Unaligned', '13', '5']
    assert [(s.get('name'), s.get('value'), s.get('id')) for s in h.iter('speed')] == [('ground', '10', 'ground'), ('swim', '40', 'swim')]


def test_ability_scores(ui):
    c = card(ui, 'Ability scores')
    scores = {'STR': '16', 'DEX': '14', 'CON': '15', 'INT': '1', 'WIS': '12', 'CHA': '3'}
    for tr in c.locator('tbody tr').all():
        name = tr.locator('th').inner_text().strip()
        tr.locator('input.num').fill(scores[name])
        if name == 'DEX':
            tr.locator('td').nth(1).locator('input[type=checkbox]').check()
    # Enter in a number applies the card.
    ui.applies(lambda: c.locator('tbody tr').last.locator('input.num').press('Enter'))
    stats = {s.get('name'): s for s in mon(ui, PIKE, 'forkjaw-pike').iter('stat')}
    assert {k: s.get('value') for k, s in stats.items()} == {k.lower(): v for k, v in scores.items()}
    assert stats['dex'].get('saveProficiency') == 'true'


def test_attributes(ui):
    c = card(ui, 'Attributes')
    ui.field('CR', scope=rows(c, 'Challenge rating')).select_option('2')
    for title in ['Skills', 'Senses', 'Resistances', 'Gear']:
        rows(c, title).get_by_role('button', name='+ Add').click()
    ui.field('Skill', scope=rows(c, 'Skills')).select_option('stealth')
    ui.field('Ability', scope=rows(c, 'Skills')).select_option('dex')
    ui.field('Feet', scope=rows(c, 'Senses')).fill('30')
    ui.field('Damage type', scope=rows(c, 'Resistances')).select_option('cold')
    ui.field('Item', scope=rows(c, 'Gear')).fill('Rusted fish hook')
    ui.field('Quantity', scope=rows(c, 'Gear')).fill('3')
    # A vulnerability added and taken out again before Apply.
    rows(c, 'Vulnerabilities').get_by_role('button', name='+ Add').click()
    rows(c, 'Vulnerabilities').get_by_title('Remove').click()
    ui.apply(c)
    a = mon(ui, PIKE, 'forkjaw-pike').find('attributes')
    assert a.find('challengeRatings/challengeRating').get('value') == '2'
    assert dict(a.find('skills/skill').attrib) == {'name': 'stealth', 'stat': 'dex', 'proficiency': 'true'}
    assert (a.find('senses/sense').get('name'), a.find('senses/sense').get('range')) == ('darkvision', '30')
    assert a.find('resistances/resistance').get('name') == 'cold'
    assert dict(a.find('gear/item').attrib) == {'name': 'Rusted fish hook', 'quantity': '3'}
    assert a.find('vulnerabilities') is None


def test_traits(ui):
    sec = section(ui, 'Traits')
    for name, text in [('Water Breathing', 'The '), ('Blood Frenzy', 'The ')]:
        ui.click_applies(sec.get_by_role('button', name='+ Trait'))
        body = open_item(item(ui, 'Traits', 'New Trait'))
        ui.field('Name', scope=body).fill(name)
        p = Prose(ui, prose_field(body, 'Description'))
        p.type(text)
        # The short noun, from Ctrl+K: "the pike".
        p.insert('pike', 'pike', "the creature's short noun")
        ui.page.keyboard.type(' can breathe only underwater.' if name == 'Water Breathing'
                              else ' has Advantage on attack rolls against any creature that doesn’t have all its Hit Points.')
        ui.apply(body)
    ui.click_applies(item(ui, 'Traits', 'Blood Frenzy').get_by_title('Move up'))
    traits = mon(ui, PIKE, 'forkjaw-pike').findall('traits/trait')
    assert [t.get('name') for t in traits] == ['Blood Frenzy', 'Water Breathing']
    assert xml_of(traits[1].find('description')) == 'The <shortNounRef/> can breathe only underwater.'


def test_attacks(ui, app):
    sec = section(ui, 'Actions')
    ui.click_applies(sec.get_by_role('button', name='+ Attack'))
    body = open_item(items(ui, 'Actions').last)
    ui.field('Name', scope=body).fill('Bite')
    ui.field('Id', scope=body).fill('bite')
    ui.field('Kind', scope=body).select_option('melee')
    ui.field('Damage dice', scope=body).fill('2d6')
    ui.field('Damage type', scope=body).select_option('piercing')
    # The hit text starts with a comma: it joins the damage with no space.
    p = Prose(ui, prose_field(body, 'Hit'))
    p.type(', and the target has the Grappled condition (escape ')
    p.new_tag('dc', 'DC (computed)', fields={'stat': 'str'})
    ui.page.keyboard.type(').')
    ui.apply(body)
    bite = mon(ui, PIKE, 'forkjaw-pike').find('actions/attack')
    assert (bite.get('name'), bite.get('id'), bite.get('damage'), bite.get('damageType')) == ('Bite', 'bite', '2d6', 'piercing')
    assert xml_of(bite.find('hit')).startswith(', and the target has the Grappled condition (escape <dc stat="str"')

    # Two more actions: one moved above the bite, one deleted (Cancel first).
    for name in ['Thrash', 'Stray']:
        ui.click_applies(sec.get_by_role('button', name='+ Action'))
        body = open_item(items(ui, 'Actions').last)
        ui.field('Name', scope=body).fill(name)
        ui.field('Id', scope=body).fill(name.lower())
        Prose(ui, prose_field(body, 'Text')).type(f'The pike {name.lower()}s.')
        ui.apply(body)
    ui.click_applies(item(ui, 'Actions', 'Thrash').get_by_title('Move up'))
    stray = item(ui, 'Actions', 'Stray')
    app.confirms_asked()
    app.answer_confirms(False)
    stray.get_by_title('Delete').click()
    assert app.confirms_asked() == ['Delete the action “Stray”?']
    ui.click_applies(stray.get_by_title('Delete'))
    acts = mon(ui, PIKE, 'forkjaw-pike').find('actions')
    assert [(k.tag, k.get('name')) for k in acts] == [('action', 'Thrash'), ('attack', 'Bite')]
    app.confirms_asked()


def test_legendary_actions(ui):
    sec = section(ui, 'Legendary Actions')
    ui.click_applies(sec.get_by_role('button', name='+ Reuse an action'))
    body = open_item(items(ui, 'Legendary Actions').last)
    ui.field('Uses', scope=body).select_option('bite')
    # Its own name is optional in the schema, but the compiler needs one to print it
    # (jyurkiw/enigma_narrative, see the plan's findings).
    ui.field('Own name', scope=body).fill('Snap')
    ui.field('Cost', scope=body).fill('2')
    ui.apply(body)
    uses = sec.locator('.table-card', has=ui.page.locator('.sb-rows-title', has_text='Legendary Action Uses'))
    ui.field('Uses', scope=uses).fill('2')
    ui.apply(uses)
    la = mon(ui, PIKE, 'forkjaw-pike').find('legendaryActions')
    assert la.find('uses/number').get('value') == '2'
    assert dict(la.find('actionRef').attrib) == {'ref': 'bite', 'name': 'Snap', 'cost': '2'}


def test_the_matriarch(ui, app):
    stats = ui.body.locator('.table-card', has=ui.page.locator('.table-card-title', has_text='Stat blocks ('))
    ui.field('New stat block name', scope=stats).fill('Forkjaw Matriarch')
    ui.field('Id', scope=stats).fill('forkjaw-matriarch')
    ui.click_applies(stats.get_by_role('button', name='+ Add after this one'))
    assert [m.get('id') for m in ui.xml(PIKE).findall('monster')] == ['forkjaw-pike', 'forkjaw-matriarch']
    stat_block(ui, PIKE, 'Forkjaw Matriarch')
    c = card(ui, 'Heading')
    ui.field('Size', scope=rows(c, 'Sizes')).select_option('Huge')
    ui.field('Type', scope=c).fill('Beast')
    ui.apply(c)
    a = card(ui, 'Attributes')
    ui.field('CR', scope=rows(a, 'Challenge rating')).select_option('5')
    ui.apply(a)
    # The lair goes with the matriarch.
    entry = card(ui, 'Entry parts')
    entry.locator('label', has_text='The Matriarch’s Pool').locator('input[type=checkbox]').check()
    ui.apply(entry)
    m = mon(ui, PIKE, 'forkjaw-matriarch')
    assert m.find('heading/sizes/size').text == 'Huge'
    assert m.find('attributes/challengeRatings/challengeRating').get('value') == '5'
    assert m.find('lairRef') is not None and m.find('lairRef').get('ref') == ui.xml(PIKE).find('lair').get('id')


def test_townsfolk(ui, app):
    stat_block(ui, TOWN, 'Toll Warden')
    stats = ui.body.locator('.table-card', has=ui.page.locator('.table-card-title', has_text='Stat blocks ('))
    for name, mid in [('Ferry Hand', 'ferry-hand'), ('Scratch', 'scratch')]:
        ui.field('New stat block name', scope=stats).fill(name)
        ui.field('Id', scope=stats).fill(mid)
        ui.click_applies(stats.get_by_role('button', name=re.compile('^\\+ Add')))
    ids = lambda: [m.get('id') for m in ui.xml(TOWN).findall('monster')]
    assert ids() == ['toll-warden', 'ferry-hand', 'scratch']
    row = lambda mid: stats.locator('.stat-row', has=ui.page.locator('.stat-id', has_text=re.compile(f'^{mid}$')))
    ui.click_applies(row('scratch').get_by_title('Move up'))
    assert ids() == ['toll-warden', 'scratch', 'ferry-hand']
    app.confirms_asked()
    ui.click_applies(row('scratch').get_by_title('Delete'))
    asked = app.confirms_asked()
    assert asked == ['Delete the stat block "scratch"?'], asked
    assert ids() == ['toll-warden', 'ferry-hand']


def test_preview(ui):
    stat_block(ui, PIKE, 'Forkjaw Pike')
    ui.side('Preview')
    panel = ui.page.locator('.preview-panel')
    expect(panel.locator('.preview-name')).to_have_text('Forkjaw Pike')
    expect(panel.locator('.preview-img').first).to_be_visible(timeout=120_000)
    expect(panel.locator('.preview-busy')).to_have_count(0, timeout=120_000)
    before = panel.locator('.preview-img').first.get_attribute('src')
    # An apply prints it again.
    c = card(ui, 'Heading')
    ui.field('Armor Class', scope=c).fill('14')
    ui.apply(c)
    expect(panel.locator('.preview-busy')).to_have_count(0, timeout=120_000)
    expect(panel.locator('.preview-img').first).not_to_have_attribute('src', before, timeout=120_000)
    expect(panel.locator('.preview-error')).to_have_count(0)
    ui.side('File structure')


def test_raw_xml(ui):
    """The stat block's raw XML card: the ferry hand's alignment, edited as XML."""
    stat_block(ui, TOWN, 'Ferry Hand')
    details = ui.body.locator('details.raw-details')
    if not details.evaluate('d => d.open'):
        details.locator('summary').click()
    raw = details.locator('.raw-card')
    area = raw.locator('textarea.raw-text')
    text = area.input_value()
    assert '<alignment>Neutral</alignment>' in text
    area.fill(text.replace('<alignment>Neutral</alignment>', '<alignment>Lawful Good</alignment>'))
    ui.apply(raw)
    assert mon(ui, TOWN, 'ferry-hand').findtext('heading/alignment') == 'Lawful Good'
