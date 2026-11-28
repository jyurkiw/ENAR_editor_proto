"""Step 11: the appendix and the back matter. A summoned-creature template in
townsfolk.xml (formulas and choices); the appendix's stat blocks (the Toll Warden from the
project, a Bandit from the SRD), their entries added by Ctrl+K library picks; a scratch
reference entry dragged to the top and deleted; credits (roles, credit lines, a line
printed as written, an ISBN, image credits); treasure parcels (coins, a gem, a magic item;
added, moved and one deleted) and a treasure link; global data values; the license part;
and a license file of the project's own, edited in the license form."""
import re
import shutil

from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'
TOWN = 'townsfolk.xml'
LICENSE = 'grand-forks-license.xml'


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def sec(ui, name):
    return ui.one(SUPP, f'//section[@name="{name}"]')


def para(ui, n=0):
    return ui.body.locator('.body-block.para').nth(n)


def rows(scope, title):
    return scope.locator('.sb-rows', has=scope.page.locator('.sb-rows-title', has_text=re.compile('^' + re.escape(title)))).first


def last_row(scope, title):
    return rows(scope, title).locator(':scope > .sb-row').last


def add_row(scope, title):
    rows(scope, title).locator(':scope > button', has_text='+ Add').click()
    return last_row(scope, title)


def back(ui):
    ui.body.get_by_role('button', name='Back to the section').click()
    expect(ui.body.locator('.block-kind-name')).to_have_text('Section')


PART_TAGS = {'Credits': 'credits', 'License': 'license', 'Treasure': 'treasure', 'Data values': 'data'}


def supplement_part(ui, label):
    """Adds a back-matter part from the supplement's page ("Add: Credits …") and opens it."""
    ui.select('supplement')
    add = ui.viewer.locator('.notice.info', has_text='Add:')
    ui.click_applies(add.get_by_role('button', name=label, exact=True))
    ui.select(PART_TAGS[label])


def test_a_template(opened, app):
    ui = opened
    ui.open(TOWN)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    stats = ui.body.locator('.table-card', has=ui.page.locator('.table-card-title', has_text='Stat blocks ('))
    ui.field('New stat block name', scope=stats).fill('Pike Spirit')
    ui.field('Id', scope=stats).fill('pike-spirit')
    ui.click_applies(stats.get_by_role('button', name=re.compile('^\\+ Add')))
    ui.page.locator('.tree .row.element[data-tag="monster"]').filter(has_text='Pike Spirit').first.click()
    ident = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Stat block'))).locator('.table-card').first
    ident.locator('label.flag', has_text='Template').locator('input').check()
    ui.apply(ident)
    assert ui.one(TOWN, '//monster[@id="pike-spirit"]').get('template') == 'true'

    formulas = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Formulas')))
    ui.click_applies(formulas.get_by_role('button', name='+ Formulas'))
    card = formulas.locator('.table-card').first
    # The new block starts with a formula; fill it in, and add a choice if there isn't one.
    f = rows(card, 'Formulas').locator(':scope > .sb-row').first
    ui.field('Id', scope=f).fill('spiritAc')
    ui.field('At least', scope=f).fill('11')
    ch = rows(card, 'Choices').locator(':scope > .sb-row').first if rows(card, 'Choices').locator(':scope > .sb-row').count() else add_row(card, 'Choices')
    ui.field('Id', scope=ch).fill('form')
    opts = rows(ch, 'Options').locator(':scope > .sb-row')
    for i, v in enumerate(['river', 'pond']):
        ui.field('Option', scope=opts.nth(i)).fill(v)
    ui.apply(card)
    fs = ui.one(TOWN, '//monster[@id="pike-spirit"]/formulas')
    assert [x.get('id') for x in fs.findall('formula')] == ['spiritAc']
    assert fs.find('formula').get('min') == '11'
    assert [o.get('value') for o in fs.find('choice').findall('option')] == ['river', 'pond']


def test_appendix_stat_blocks(ui):
    ui.open(SUPP)
    ui.select('section', name='creatures')
    row = para(ui)
    p = Prose(ui, row)
    p.type('The Toll Warden keeps order; a Bandit or two works the road; the Pike Spirit haunts the weir.')
    ui.apply(row)
    for words, query, label, sub in [('Toll Warden', 'warden', 'Toll Warden', 'project monster'),
                                     ('Bandit', 'bandit', 'Bandit', 'SRD monster'),
                                     ('Pike Spirit', 'spirit', 'Pike Spirit', 'project monster')]:
        p.select(words)
        ui.applies(lambda: p.insert(query, label, sub))
        offer = p.editor.locator('.pill-offer')
        if offer.count():
            offer.get_by_role('button', name='No thanks').click()
        ui.apply(row)
    mons = {m.get('id'): m for m in ui.xml(SUPP).findall('reference/monster')}
    assert {'toll-warden', 'bandit', 'pike-spirit'} <= set(mons), list(mons)
    assert mons['toll-warden'].get('file') == '../townsfolk.xml'
    assert mons['bandit'].get('file', '').endswith('.xml'), dict(mons['bandit'].attrib)    # the SRD's file

    # An inline monster from Ctrl+K: "Brisk (Toll Warden, CR …)", typed on through.
    p.type(' The warden on duty is usually ')
    p.new_tag('inline', 'Inline monster', fields={'ref': 'toll-warden', 'display': 'Brisk'})
    ui.page.keyboard.type(', who never takes a bribe.')
    ui.apply(row)
    p_xml = xml_of(sec(ui, 'creatures').find('p'))
    assert 'is usually <monster ref="toll-warden" display="Brisk"/>, who never takes a bribe.' in p_xml, p_xml

    for mid in ['toll-warden', 'bandit']:
        ui.applies(lambda: ui.body.locator('.body-block').last.locator('select.add-menu').select_option('monsterStatblock'))
        ui.field('Monster', scope=ui.body).select_option(mid)
        if ui.body.get_by_role('button', name='Apply', exact=True).is_enabled():
            ui.apply(ui.body)
        back(ui)
    assert [b.get('ref') for b in sec(ui, 'creatures').findall('monsterStatblock')] == ['toll-warden', 'bandit']


def test_reference_reorder_and_delete(ui, app):
    ui.select('reference')
    ui.body.get_by_role('button', name='+ Add ▾').click()
    ui.body.locator('.add-menu button', has=ui.page.locator('.ref-kind', has_text='data')).click()
    card = ui.body.locator('[id^="ref-card-new:"]').last
    expect(card.locator('input[data-attr=id]')).to_be_focused()
    card.locator('input[data-attr=id]').fill('scratch')
    card.locator('input[data-attr=value]').fill('nothing')
    ui.apply(card)
    ids = lambda: [e.get('id') for e in ui.xml(SUPP).find('reference')]
    assert ids()[-1] == 'scratch'
    # Dragged onto the top half of the card above it on screen (a short drag: a long one
    # scrolls the page under the pointer before the drag starts).
    scratch = ui.body.locator('[id="ref-card-data:scratch"]')
    cards = ui.body.locator('.ref-card[id^="ref-card-"]')
    keys = cards.evaluate_all('cs => cs.map(c => c.id.slice("ref-card-".length))')
    above = keys[keys.index('data:scratch') - 1]
    target = ui.body.locator(f'[id="ref-card-{above}"]')
    ui.applies(lambda: scratch.locator('.ref-head').drag_to(target.locator('.ref-head'), target_position={'x': 40, 'y': 2}))
    got = ids()
    assert got.index('scratch') == got.index(above.split(':', 1)[1]) - 1, (above, got)
    app.confirms_asked()
    app.answer_confirms(False)
    scratch.get_by_title('Delete').click()
    assert app.confirms_asked() == ['Delete <data id="scratch"> from <reference>?']
    ui.click_applies(scratch.get_by_title('Delete'))
    app.confirms_asked()
    assert 'scratch' not in ids()

    # An existing entry changed: the map's artist.
    card = ui.body.locator('[id="ref-card-image:img-map"]')
    if not card.locator('input[data-attr=credit]').count():
        card.locator('.ref-head').click()
    card.locator('input[data-attr=credit]').fill('Slop Art Studio (maps)')
    ui.apply(card)
    assert ui.one(SUPP, '//reference/image[@id="img-map"]').get('credit') == 'Slop Art Studio (maps)'


def test_credits(ui):
    supplement_part(ui, 'Credits')
    c = ui.body.locator('.table-card').first
    # Author: the role chapter credits (@credit) name. No Artist role: artist credits
    # would replace <imageCredits/>, which lists each image's artist.
    for name, plural in [('Author', ''), ('Editor', 'Editors')]:
        r = add_row(c, 'Roles')
        ui.field('Role', scope=r).fill(name)
        if plural:
            ui.field('Plural', scope=r).fill(plural)
    for name, role in [('Test Author', 'Author'), ('A. N. Other', 'Editor'), ('Scratch Name', 'Editor')]:
        r = add_row(c, 'Credits')
        ui.field('Name', scope=r).fill(name)
        ui.field('Role', scope=r).fill(role)
    last_row(c, 'Credits').get_by_title('Remove').click()        # one too many
    ui.field('Line', scope=add_row(c, 'Lines printed as written')).fill('Thanks to everyone who crossed the bridge.')
    r = add_row(c, 'ISBNs')
    ui.field('ISBN', scope=r).fill('978-1-23456-789-7')
    ui.field('Format', scope=r).select_option('pdf')
    c.locator('label.flag', has_text='Image credits').locator('input').check()
    ui.apply(c)
    cr = ui.one(SUPP, '/supplement/credits')
    assert [(r.get('name'), r.get('plural')) for r in cr.findall('role')] == [('Author', None), ('Editor', 'Editors')]
    assert [(r.get('name'), r.get('role')) for r in cr.findall('credit')] == [
        ('Test Author', 'Author'), ('A. N. Other', 'Editor')]
    assert cr.findtext('verbatimCredit') == 'Thanks to everyone who crossed the bridge.'
    assert (cr.find('isbn').get('value'), cr.find('isbn').get('format')) == ('978-1-23456-789-7', 'pdf')
    assert cr.find('imageCredits') is not None


def test_treasure(ui, app):
    supplement_part(ui, 'Treasure')
    parcels = ui.body.locator('.table-card', has=ui.page.locator('button[title="Delete this parcel"]'))
    while parcels.count() < 3:
        ui.click_applies(ui.body.get_by_role('button', name='+ Parcel'))
    # The first: the toll box. The second: the matriarch's hoard. The third goes.
    first = parcels.nth(0)
    ui.field('GP', scope=first).fill('25')
    ui.field('SP', scope=first).fill('140')
    ui.apply(first)
    hoard = parcels.nth(1)
    ui.field('GP', scope=hoard).fill('60')
    gem = add_row(hoard, 'Gems')
    ui.field('What', scope=gem).fill('a river pearl')
    ui.field('Worth (cp)', scope=gem).fill('10000')
    item = add_row(hoard, 'Magic items')
    ui.field('Item entry', scope=item).fill('ferrymans-lantern')
    ui.apply(hoard)
    ui.click_applies(hoard.locator('.table-card-title').get_by_title('Move up'))
    app.confirms_asked()
    ui.click_applies(parcels.nth(2).get_by_title('Delete this parcel'))
    assert len(app.confirms_asked()) == 1
    t = ui.one(SUPP, '/supplement/treasure')
    ps = t.findall('parcel')
    assert len(ps) == 2
    assert ps[0].find('coins/gp').get('count') == '60' and ps[0].find('gems/gem').text == 'a river pearl'
    assert ps[0].find('magicItems/item').get('ref') == 'ferrymans-lantern'
    assert (ps[1].find('coins/gp').get('count'), ps[1].find('coins/sp').get('count')) == ('25', '140')

    # The hoard, printed in the pike attack.
    ui.select('section', name='pike-attack')
    ui.applies(lambda: ui.body.locator('.body-block').last.locator('select.add-menu').select_option('parcelLink'))
    hoard_no = ps[0].get('number')          # parcels keep their numbers when they move
    assert hoard_no, etree.tostring(ps[0])
    ui.field('Parcel', scope=ui.body).select_option(hoard_no)
    if ui.body.get_by_role('button', name='Apply', exact=True).is_enabled():
        ui.apply(ui.body)
    assert sec(ui, 'pike-attack').find('parcelLink').get('number') == hoard_no
    back(ui)
    # And the toll box, at the bridge: every parcel is printed somewhere.
    ui.select('section', name='toll-bridge')
    ui.applies(lambda: ui.body.locator('.body-block').last.locator('select.add-menu').select_option('parcelLink'))
    box_no = ps[1].get('number')
    ui.field('Parcel', scope=ui.body).select_option(box_no)
    if ui.body.get_by_role('button', name='Apply', exact=True).is_enabled():
        ui.apply(ui.body)
    assert sec(ui, 'toll-bridge').find('parcelLink').get('number') == box_no
    back(ui)


def test_data_values(ui):
    supplement_part(ui, 'Data values')
    c = ui.body.locator('.table-card').first
    r = add_row(c, 'Global values')
    ui.field('Name', scope=r).fill('fordDepth')
    ui.field('Value', scope=r).fill('4 feet')
    ui.apply(c)
    v = ui.one(SUPP, '/supplement/data/global/value')
    assert (v.get('name'), v.get('value')) == ('fordDepth', '4 feet')


def test_license_part(ui):
    supplement_part(ui, 'License')
    c = ui.body.locator('.table-card').first
    ui.field('Date', scope=c).fill('2026')
    ui.field('Owner', scope=c).fill('The Grand Forks Test Suite')
    ui.apply(c)
    lic = ui.one(SUPP, '/supplement/license')
    assert (lic.get('file'), lic.get('date'), lic.get('ownedBy')) == ('dmsguild-license.xml', '2026', 'The Grand Forks Test Suite')
    order = [c.get('name') for c in ui.xml(SUPP).findall('chapterOrder/ch')]
    assert order == ['introduction', 'tollside', 'millward', 'wedge', 'creatures']


def test_a_license_file(ui, app):
    """A copy of the DMs Guild notice, in the project, edited in the license form."""
    src = app.data_repo / 'schemas' / 'dmsguild-license.xml'
    text = src.read_text(encoding='utf-8').replace('xsi:noNamespaceSchemaLocation="license-template-schema.xsd"',
                                                    'xsi:noNamespaceSchemaLocation="../../../schemas/license-template-schema.xsd"')
    (app.project_dir / LICENSE).write_text(text, encoding='utf-8')
    ui.file_menu('', 'Refresh')
    ui.open(LICENSE)
    ui.select('license')
    c = ui.body.locator('.table-card').first
    ui.field('Copied on', scope=c).fill('2026-11-19')
    notice = rows(c, 'Notice').locator(':scope > .sb-row')
    n = notice.count()
    add = rows(c, 'Notice').locator(':scope > button', has_text='+ Add')
    add.click()
    Prose(ui, notice.nth(n)).type('This copy belongs to the Grand Forks test project.')
    ui.apply(c)
    root = ui.xml(LICENSE)
    assert root.get('download_date') == '2026-11-19'
    assert [p.xpath('string(.)').strip() for p in root.findall('p')][-1] == 'This copy belongs to the Grand Forks test project.'
    assert len(root.findall('p')) == n + 1
