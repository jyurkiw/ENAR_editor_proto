"""Step 06: Tollside. The toll table, made from a table link (headers added, filled and
moved, rows added, moved and deleted, a cell's spans set and cleared) and placed by that
link; a "Toll Disputes" sidebar from a sidebar link; the warden's dialog box with a speaker
and a trigger; the bridge toll made a variable and used again through Ctrl+K; the toll
bridge picture.

Two table gaps shape this step (see the plan's findings): there's no way to add a footer
or to delete a cell, so the new table's first row (a cell over once it has headers) is
replaced rather than fixed, and the spans are cleared again so every row still matches
its headers."""
from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'
TOLLS = [('On foot', '1 cp', 'Children cross free'),
         ('Cart or wagon', '5 cp', 'Per axle'),
         ('Herd animals', '1 cp a head', 'The warden counts them')]


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def sec(ui, name):
    return ui.one(SUPP, f'//section[@name="{name}"]')


def table(ui):
    return ui.one(SUPP, '//chapter[@name="tollside"]/table')


def body_text(ui):
    return [[''.join(td.itertext()) for td in tr.findall('td')] for tr in table(ui).findall('tableBody/tr')]


def para(ui, n=0):
    return ui.body.locator('.body-block.para').nth(n)


def headers(ui):
    return ui.body.locator('.th-row')


def rows(ui):
    return ui.body.locator('.row-card')


def cell(ui, row, col):
    return rows(ui).nth(row).locator('.cell').nth(col)


def test_bridge_prose(opened):
    ui = opened
    ui.open(SUPP)
    ui.select('section', name='toll-bridge')
    row = para(ui)
    Prose(ui, row).type('Every traveller pays the toll of 2 cp at the north end of the bridge.')
    ui.apply(row)


def test_the_toll_is_a_variable(ui):
    row = para(ui)
    p = Prose(ui, row)
    menu = p.menu('2 cp', select=True)
    p.menu_item(menu, 'Turn into variable').click()
    form = p.editor.locator('.insert-panel')
    name = form.locator('label.pill-field', has_text='Name').locator('input')
    expect(name).to_have_value('cp')                  # made from the value's words
    expect(form).to_contain_text('Value: “2 cp”')
    # Not letters only: refused, with the reason.
    name.fill('toll2')
    form.get_by_role('button', name='Make variable').click()
    expect(form.locator('.hint.bad')).to_contain_text('Letters only')
    name.fill('bridgeToll')
    ui.click_applies(form.get_by_role('button', name='Make variable'))
    assert dict(ui.one(SUPP, '//reference/data').attrib) == {'id': 'bridgeToll', 'value': '2 cp'}
    ui.apply(row)
    assert xml_of(sec(ui, 'toll-bridge').find('p')) == \
        'Every traveller pays the toll of <dataRef ref="bridgeToll" value="value"/> at the north end of the bridge.'
    expect(p.chips()).to_have_text(['2 cp'])


def test_the_toll_table(ui):
    ui.applies(lambda: para(ui).locator('select.add-menu').select_option('tableLink'))
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'tableLink')
    expect(ui.field('Table', scope=ui.body)).to_have_value('new-table')
    ui.body.get_by_role('button', name='Open the table').click()
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'table')

    info = ui.body.locator('.table-card').first
    ui.field('Id *', scope=info).fill('toll-rates')
    ui.field('Name', scope=info).fill('Bridge Tolls')
    expect(info.locator('.warn-text')).to_have_text('Apply also renames the 1 <tableLink>(s) that place it.')
    ui.apply(info)
    assert (table(ui).get('id'), table(ui).findtext('title')) == ('toll-rates', 'Bridge Tolls')
    assert ui.one(SUPP, '//section[@name="toll-bridge"]/tableLink').get('id') == 'toll-rates'

    # Three headers: one, one at the end, one inserted between; filled, then the middle
    # one moved down.
    ui.click_applies(ui.body.get_by_role('button', name='+ Add a header'))
    ui.click_applies(ui.body.get_by_role('button', name='+ Header at the end'))
    ui.click_applies(headers(ui).nth(0).get_by_title('Insert a header below (adds a column)'))
    for i, text in enumerate(['Traveller', 'Notes', 'Toll']):
        Prose(ui, headers(ui).nth(i)).type(text)
        ui.apply(headers(ui).nth(i))
    ui.click_applies(headers(ui).nth(1).get_by_title('Move down (with its column)'))
    assert [th.text for th in table(ui).findall('header/th')] == ['Traveller', 'Toll', 'Notes']
    # A fourth, deleted again with its column.
    ui.click_applies(ui.body.get_by_role('button', name='+ Header at the end'))
    ui.click_applies(headers(ui).nth(3).get_by_title('Delete (with its column)'))
    assert [th.text for th in table(ui).findall('header/th')] == ['Traveller', 'Toll', 'Notes']

    # The new table's row predates the headers and is a cell over; rows added now fit.
    expect(rows(ui).first.locator('.hint.bad')).to_have_text("cell count doesn't match the headers")
    for i, values in enumerate(TOLLS):
        ui.click_applies(rows(ui).nth(i).get_by_role('button', name='+ row below'))
        expect(rows(ui).nth(i + 1).locator('.cell')).to_have_count(3)
        for col, text in enumerate(values):
            c = cell(ui, i + 1, col)
            Prose(ui, c).type(text)
            ui.apply(c)
    ui.click_applies(rows(ui).first.get_by_title('Delete row'))
    assert body_text(ui) == [list(v) for v in TOLLS]

    # Move the herd row up, and back.
    ui.click_applies(rows(ui).nth(2).get_by_title('Move up'))
    assert body_text(ui)[1][0] == 'Herd animals'
    ui.click_applies(rows(ui).nth(1).get_by_title('Move down'))
    assert body_text(ui) == [list(v) for v in TOLLS]

    # Spans: the cart's notes across two columns, then back.
    c = cell(ui, 1, 2)
    c.get_by_role('button', name='Spans').click()
    c.locator('label.span-field', has_text='Cols').locator('input').fill('2')
    ui.click_applies(c.get_by_role('button', name='Apply spans'))
    assert table(ui).findall('tableBody/tr')[1].findall('td')[2].get('colspan') == '2'
    expect(c.locator('.span-badge')).to_be_visible()
    c.get_by_role('button', name='Spans').click()
    c.locator('label.span-field', has_text='Cols').locator('input').fill('')
    ui.click_applies(c.get_by_role('button', name='Apply spans'))
    assert table(ui).findall('tableBody/tr')[1].findall('td')[2].get('colspan') is None

    # The toll on foot is the bridge toll: a data ref in a cell, through Ctrl+K.
    c = cell(ui, 0, 1)
    p = Prose(ui, c)
    p.select('1 cp')
    p.insert('2 cp', '2 cp', 'data entry')
    ui.apply(c)
    assert xml_of(table(ui).find('tableBody/tr/td[2]')) == '<dataRef ref="bridgeToll" value="value"/>'
    expect(ui.body.locator('.row-card.mismatch')).to_have_count(0)


def test_a_sidebar(ui):
    ui.select('section', name='coaching-inn')
    row = para(ui)
    Prose(ui, row).type('The Copper Kettle is where the toll disputes end up.')
    ui.apply(row)
    ui.applies(lambda: row.locator('select.add-menu').select_option('sidebarLink'))
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'sidebarLink')
    ui.select('sidebar')
    expect(ui.body.locator('.block-kind-name')).to_have_text('Sidebar')
    expect(ui.body.locator('.uses')).to_have_text('placed by 1 <sidebarLink>')
    head = ui.body.locator('.block-header')
    ui.field('Title', scope=head).fill('Toll Disputes')
    ui.field('Id', scope=head).fill('toll-disputes')
    expect(head.locator('.warn-text')).to_have_text('Apply also renames the 1 <sidebarLink>(s) that place it.')
    ui.apply(head)
    row = para(ui)
    Prose(ui, row).type('A traveller who will not pay is held until the Toll Warden hears the case.')
    ui.apply(row)
    sb = ui.one(SUPP, '//chapter[@name="tollside"]/sidebar')
    assert (sb.get('id'), sb.findtext('title'), sb.findtext('p')) == \
        ('toll-disputes', 'Toll Disputes', 'A traveller who will not pay is held until the Toll Warden hears the case.')
    assert ui.one(SUPP, '//section[@name="coaching-inn"]/sidebarLink').get('id') == 'toll-disputes'


def test_the_wardens_dialog(ui):
    ui.select('section', name='wardens-house')
    row = para(ui)
    Prose(ui, row).type('The Toll Warden lives above the gatehouse.')
    ui.apply(row)
    ui.applies(lambda: row.locator('select.add-menu').select_option('dialog'))
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'dialog')
    speaker = ui.field('Speaker', scope=ui.body)
    speaker.fill('Toll Warden')
    ui.applies(lambda: speaker.press('Enter'))
    lines = ui.body.locator('.body-block')
    for i, text in [(0, 'If a traveller argues about the toll:'), (1, 'Two coppers, friend. The river does not cross itself.')]:
        Prose(ui, lines.nth(i)).type(text)
        ui.apply(lines.nth(i))
    ui.click_applies(lines.nth(1).get_by_role('button', name='+ ¶ below'))
    Prose(ui, lines.nth(2)).type('And no, the Sallow does not count as a road.')
    ui.apply(lines.nth(2))
    d = ui.one(SUPP, '//section[@name="wardens-house"]/dialog')
    assert d.get('speaker') == 'Toll Warden'
    assert d.findtext('trigger') == 'If a traveller argues about the toll:'
    assert [xml_of(p) for p in d.findall('p')] == [
        'Two coppers, friend. The river does not cross itself.',
        # The river is a noun now, so its mention was pillable; it's typed as plain text.
        'And no, the Sallow does not count as a road.']

    # A stray line: dragged above the others, then deleted.
    ui.click_applies(lines.nth(2).get_by_role('button', name='+ ¶ below'))
    Prose(ui, lines.nth(3)).type('A stray line.')
    ui.apply(lines.nth(3))
    ui.applies(lambda: lines.nth(3).locator('.grip').drag_to(lines.nth(1), target_position={'x': 40, 'y': 4}))
    lines_now = lambda: [xml_of(p) for p in ui.one(SUPP, '//section[@name="wardens-house"]/dialog').findall('p')]
    assert lines_now()[0] == 'A stray line.', lines_now()
    ui.click_applies(lines.nth(1).get_by_title('Delete paragraph'))
    assert lines_now() == ['Two coppers, friend. The river does not cross itself.', 'And no, the Sallow does not count as a road.']


def test_delete_a_box(ui, app):
    ui.select('section', name='wardens-house')
    ui.applies(lambda: para(ui).locator('select.add-menu').select_option('speakAloud'))
    assert ui.one(SUPP, '//section[@name="wardens-house"]').find('speakAloud') is not None
    app.confirms_asked()
    ui.click_applies(ui.body.get_by_role('button', name='Delete read-aloud box'))
    app.confirms_asked()
    assert ui.one(SUPP, '//section[@name="wardens-house"]').find('speakAloud') is None


def test_the_bridge_picture(ui):
    ui.select('section', name='toll-bridge')
    last = ui.body.locator('.body-block').last
    ui.applies(lambda: last.locator('select.add-menu').select_option('image'))
    expect(ui.page.locator('.tree .row.selected')).to_have_attribute('data-tag', 'image')
    ui.field('Image', scope=ui.body).select_option('img-toll-bridge')
    ui.field('Placement', scope=ui.body).select_option('top')
    ui.apply(ui.body)
    img = sec(ui, 'toll-bridge').find('image')
    assert dict(img.attrib) == {'id': 'img-toll-bridge', 'placement': 'top'}
    kids = [k.tag for k in sec(ui, 'toll-bridge')]
    assert kids == ['title', 'p', 'tableLink', 'image'], kids
