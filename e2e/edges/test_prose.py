"""The prose editor, past what the story does: a tag picked in Ctrl+K while the paragraph
has unapplied text (it used to apply the paragraph and close the tag's form: issue 8),
and Escape putting the caret back in the box (issue 17)."""
from lxml import etree
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'


def xml_of(el):
    s = etree.tostring(el, encoding='unicode', with_tail=False)
    return s[s.index('>') + 1:s.rindex('<')]


def test_typing_on_through_a_tag(edge, app):
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    ui.click_applies(ui.body.locator('.body-block.para').first.get_by_role('button', name='+ ¶ below'))
    row = ui.body.locator('.body-block.para').nth(1)
    p = Prose(ui, row)
    n = len(app.changes())
    p.type('A wet miller is ')
    p.new_tag('cond', 'Condition', text='prone')
    ui.page.keyboard.type(' until helped up, and takes a ')
    p.new_tag('ability', 'Ability check', fields={'stat': 'con', 'skill': 'survival', 'dc': '10'})
    ui.page.keyboard.type(' to stop shivering.')
    ui.page.wait_for_timeout(300)
    assert len(app.changes()) == n, 'something applied before Apply'
    ui.apply(row)
    got = xml_of(ui.one(SUPP, '//section[@name="mills"]').findall('p')[1])
    assert got == ('A wet miller is <condition>prone</condition> until helped up, and takes a '
                   '<skillcheck stat="con" skill="survival" dc="10"/> to stop shivering.'), got


def test_escape_puts_the_caret_back(edge):
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    row = ui.body.locator('.body-block.para').first
    p = Prose(ui, row)
    p.focus_end()
    panel = p.generator('cond')
    ui.page.keyboard.press('Escape')
    expect(panel).to_have_count(0)
    expect(p.box).to_be_focused()
    ui.page.keyboard.type(' Typed after Escape.')
    expect(p.box).to_contain_text('Typed after Escape.')
    row.get_by_role('button', name='Revert').click()


def test_escape_closes_the_add_menu(edge):
    ui = edge
    ui.open(SUPP)
    ui.select('reference')
    ui.body.get_by_role('button', name='+ Add ▾').click()
    menu = ui.body.locator('.add-menu')
    expect(menu).to_be_visible()
    ui.page.keyboard.press('Escape')
    expect(menu).to_have_count(0)
    expect(ui.page.locator('.menu-backdrop')).to_have_count(0)
