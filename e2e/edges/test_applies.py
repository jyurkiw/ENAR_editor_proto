"""Applies that don't go through, and edits that are thrown away: a refused apply leaves the
file alone byte for byte and says why; Revert drops unapplied edits; the Source view shows
what Apply would write; a delete answered Cancel keeps what it would have deleted."""
import re

import pytest
from playwright.sync_api import expect

from pages.prose import Prose

SUPP = 'grand-forks/supplement.xml'
PIKE = 'grand-forks/forkjaw-pike.xml'
TOWN = 'townsfolk.xml'


def heading_card(ui):
    sec = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Heading')))
    return sec.locator('.table-card').first


def test_a_refused_apply_leaves_the_file_alone(edge):
    ui = edge
    ui.open(PIKE)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    before = (ui.root / PIKE).read_bytes()
    card = heading_card(ui)
    ui.field('Armor Class', scope=card).fill('lots')
    changes = ui.applies(lambda: card.get_by_role('button', name='Apply', exact=True).click(), status='Invalid')
    assert changes[-1]['result']['status'] == 'Invalid'
    assert (ui.root / PIKE).read_bytes() == before
    expect(card).to_have_class(re.compile(r'\bfailed\b'))
    badge = ui.page.locator('.problems .badge')
    expect(badge).not_to_have_text('0')
    ui.page.locator('.problems').click()
    expect(ui.page.locator('.problems')).to_contain_text(re.compile(r'Ln \d+'))
    expect(ui.page.locator('.problems')).to_contain_text("'lots' is not a valid value")
    # Revert: the draft goes, and with it the refusal.
    card.get_by_role('button', name='Revert').click()
    expect(ui.field('Armor Class', scope=card)).not_to_have_value('lots')
    expect(card).not_to_have_class(re.compile(r'\b(failed|dirty)\b'))
    assert (ui.root / PIKE).read_bytes() == before


def test_revert_and_the_unapplied_mark(edge):
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='mills')
    before = (ui.root / SUPP).read_bytes()
    row = ui.body.locator('.body-block.para').first
    Prose(ui, row).type(' Some words to throw away.')
    expect(row.locator('.state-badge')).to_have_text('unapplied')
    row.get_by_role('button', name='Revert').click()
    expect(row.locator('.state-badge')).to_have_count(0)
    expect(row.locator('.chip-box')).not_to_contain_text('Some words to throw away.')
    assert (ui.root / SUPP).read_bytes() == before


def test_source_shows_what_apply_would_write(edge):
    """For a chapter's unapplied edits, and a paragraph's (issue 16: paragraphs, tables,
    lists, boxes and reference cards used to show the file as written)."""
    ui = edge
    ui.open(SUPP)
    ui.select('chapter', name='millward')
    ui.field('Title').fill('Millward, on the East Fork')
    ui.mode('Source')
    expect(ui.viewer.locator('.source-note')).to_have_text('Showing the text after Apply (not yet written).')
    expect(ui.viewer.locator('pre.code')).to_contain_text('<chapterTitle>Millward, on the East Fork</chapterTitle>')
    ui.mode('Form')
    ui.viewer.get_by_role('button', name='Revert').first.click()
    expect(ui.field('Title')).to_have_value('Millward')
    ui.mode('Source')
    expect(ui.viewer.locator('.source-note')).to_have_count(0)
    ui.mode('Form')

    ui.select('section', name='mills')
    row = ui.body.locator('.body-block.para').first
    Prose(ui, row).type(' Words not yet applied.')
    ui.mode('Source')
    expect(ui.viewer.locator('.source-note')).to_have_text('Showing the text after Apply (not yet written).')
    expect(ui.viewer.locator('pre.code')).to_contain_text('Words not yet applied.')
    ui.mode('Form')
    row.get_by_role('button', name='Revert').click()


def test_source_shows_card_drafts(edge):
    """The rest of issue 16: a stat block card's and a family part's unapplied edits show
    in the Source view too."""
    ui = edge
    ui.open(PIKE)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    card = heading_card(ui)
    ui.field('Armor Class', scope=card).fill('17')
    ui.mode('Source')
    expect(ui.viewer.locator('.source-note')).to_have_text('Showing the text after Apply (not yet written).')
    expect(ui.viewer.locator('pre.code')).to_contain_text('<armorClass>17</armorClass>')
    ui.mode('Form')
    card.get_by_role('button', name='Revert').click()

    # A family part: the image's alt text.
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="image"]').first.click()
    img = ui.body.locator('.table-card', has=ui.page.locator('.field-label', has_text='Alt text'))
    ui.field('Alt text', scope=img).fill('A pike, not yet applied')
    ui.mode('Source')
    expect(ui.viewer.locator('.source-note')).to_have_text('Showing the text after Apply (not yet written).')
    expect(ui.viewer.locator('pre.code')).to_contain_text('alt="A pike, not yet applied"')
    ui.mode('Form')
    img.get_by_role('button', name='Revert').click()
    ui.mode('Source')
    expect(ui.viewer.locator('.source-note')).to_have_count(0)
    ui.mode('Form')


def test_the_source_view(edge):
    ui = edge
    ui.open(SUPP)
    ui.select('section', name='weir')
    ui.mode('Source')
    code = ui.viewer.locator('pre.code')
    expect(code).to_contain_text('<section name="weir">')
    # Re-indented: the element starts at the left margin, whatever its depth in the file.
    assert code.inner_text().startswith('<section name="weir">')
    assert code.locator('span').count() > 5          # highlighted
    where = ui.viewer.get_by_text(re.compile(r'^lines? \d+'))
    first = where.first.inner_text()
    ui.select('section', name='market')
    expect(code).to_contain_text('<section name="market">')
    expect(where.first).not_to_have_text(first)
    ui.mode('Form')
    # Expand all and Collapse all in the tree.
    tools = ui.page.locator('.tree-tools')
    tools.get_by_role('button', name='Collapse all').click()
    expect(ui.page.locator('.tree .row.element[data-tag="section"]')).to_have_count(0)
    tools.get_by_role('button', name='Expand all').click()
    expect(ui.page.locator('.tree .row.element[data-tag="section"]').first).to_be_visible()


# Each delete answered Cancel: (file, how to get to it, the button, the message's start).
def _para(ui):
    ui.open(SUPP)
    ui.select('section', name='weir')
    return ui.body.locator('.body-block.para').first.get_by_title('Delete paragraph')


def _list(ui):
    ui.open(SUPP)
    ui.select('section', name='three-pieces')      # lists aren't in the tree: open it from its section
    ui.body.locator('.body-block', has=ui.page.locator('.ro-tag', has_text='List')).get_by_role('button', name='Open').click()
    return ui.body.get_by_role('button', name='Delete list')


def _chapter(ui):
    ui.open(SUPP)
    ui.select('chapter', name='wedge')
    return ui.body.get_by_role('button', name='Delete chapter')


def _ref(ui):
    ui.open(SUPP)
    ui.select('reference')
    return ui.body.locator('[id="ref-card-noun:copperway"]').get_by_title('Delete')


def _parcel(ui):
    ui.open(SUPP)
    ui.select('treasure')
    return ui.body.get_by_title('Delete this parcel').first


def _stat_block(ui):
    ui.open(TOWN)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    stats = ui.body.locator('.table-card', has=ui.page.locator('.table-card-title', has_text='Stat blocks ('))
    return stats.locator('.stat-row').last.get_by_title('Delete')


def _family_part(ui):
    ui.open(PIKE)
    ui.select('monsters')
    return ui.body.locator('.stat-row', has=ui.page.locator('.part-kind')).first.get_by_title('Delete')


def _action(ui):
    ui.open(PIKE)
    ui.side('File structure')
    ui.page.locator('.tree .row.element[data-tag="monster"]').first.click()
    sec = ui.body.locator('details.sb-section', has=ui.page.locator('summary', has_text=re.compile('^Actions')))
    return sec.locator('details.sb-item').first.locator('summary').get_by_title('Delete')


CANCELS = [(SUPP, _para), (SUPP, _list), (SUPP, _chapter), (SUPP, _ref), (SUPP, _parcel),
           (TOWN, _stat_block), (PIKE, _family_part), (PIKE, _action)]


@pytest.mark.parametrize('rel, find', CANCELS, ids=[f.__name__.strip('_') for _, f in CANCELS])
def test_cancel_keeps_it(edge, app, rel, find):
    ui = edge
    button = find(ui)
    before = (ui.root / rel).read_bytes()
    n = len(app.changes())
    app.confirms_asked()
    app.answer_confirms(False)
    button.click()
    asked = app.confirms_asked()
    assert len(asked) == 1 and asked[0].startswith('Delete'), asked
    ui.page.wait_for_timeout(300)
    assert len(app.changes()) == n
    assert (ui.root / rel).read_bytes() == before


def test_add_menus_sit_in_their_rows(edge):
    """Each "+ other below…" select is drawn inside its own row (or the tools bar), not
    over the next row's grip and text box: a dropdown rule for the reference form's menu
    once positioned them all absolutely."""
    ui = edge
    ui.open(SUPP)
    ui.select('section', label='intro-opening')
    out = ui.body.locator('select.add-menu').evaluate_all("""sels => sels.map(s => {
        const row = s.closest('.body-block, .body-tools');
        const a = s.getBoundingClientRect(), b = row.getBoundingClientRect();
        const inside = a.top >= b.top - 1 && a.bottom <= b.bottom + 1 && a.left >= b.left - 1 && a.right <= b.right + 1;
        return inside ? null : `${row.className}: select ${Math.round(a.top)}-${Math.round(a.bottom)}, row ${Math.round(b.top)}-${Math.round(b.bottom)}`;
    }).filter(x => x)""")
    assert ui.body.locator('select.add-menu').count() > 3
    assert out == [], out
