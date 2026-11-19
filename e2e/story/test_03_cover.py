"""Step 03: the cover and front matter. The skeleton's cover is removed and a new one added,
the <reference> block made and the art's images entered in it, then the cover filled in:
title, cover art, footer, text colour, both logos and the contents-page logo.

The supplement's <productDescription> has no form yet, so it isn't written here."""
from playwright.sync_api import expect

SUPP = 'grand-forks/supplement.xml'

# id, file, title, artist ('' leaves the picture uncredited, for the datasheet's list)
IMAGES = [
    ('img-cover', 'art/cover.png', 'Grand Forks', 'Slop Art Studio'),
    ('img-map', 'art/grand-forks-map.png', 'Map of Grand Forks', 'Slop Art Studio'),
    ('img-toll-bridge', 'art/toll-bridge.png', 'The Toll Bridge', 'A. N. Other'),
    ('img-lantern', 'art/lantern.png', "Ferryman's Lantern", ''),
]


def test_remove_the_skeleton_cover(opened, app):
    ui = opened
    ui.open(SUPP)
    ui.select('cover')
    remove = ui.body.get_by_role('button', name='Remove cover')

    app.answer_confirms(False)
    remove.click()
    assert app.confirms_asked() == ['Remove the cover from this supplement? Its title, art and footer lines are deleted from the file.']
    assert ui.xml(SUPP).find('cover') is not None

    app.answer_confirms(True)
    ui.click_applies(remove)
    assert app.confirms_asked()
    assert ui.xml(SUPP).find('cover') is None


def test_add_a_cover_and_the_reference_block(ui):
    ui.select('supplement')
    notice = ui.viewer.locator('.notice', has_text='This supplement has no cover.')
    ui.click_applies(notice.get_by_role('button', name='Add a cover'))
    expect(notice).to_have_count(0)
    assert ui.xml(SUPP).find('cover') is not None

    ui.select('supplement')         # the apply selected the new cover
    ui.click_applies(ui.viewer.get_by_role('button', name='Create <reference>'))
    assert ui.xml(SUPP).find('reference') is not None


def test_images_in_the_reference_block(ui):
    ui.select('reference')
    for id_, src, title, artist in IMAGES:
        ui.body.get_by_role('button', name='+ Add ▾').click()
        ui.body.locator('.add-menu button', has=ui.page.locator('.ref-kind', has_text='image')).click()
        card = ui.body.locator('[id^="ref-card-new:"]').last
        expect(card.locator('input[data-attr=id]')).to_be_focused()       # the card focuses it a frame later
        card.locator('input[data-attr=id]').fill(id_)
        card.locator('input[data-attr=src]').fill(src)
        card.locator('input[data-attr=name]').fill(title)
        if artist:
            card.locator('input[data-attr=credit]').fill(artist)
        ui.apply(card)
        expect(ui.body.locator(f'[id="ref-card-image:{id_}"]')).to_be_visible()

    images = {i.get('id'): i for i in ui.xml(SUPP).findall('reference/image')}
    assert list(images) == [i[0] for i in IMAGES]
    assert images['img-cover'].get('src') == 'art/cover.png'
    assert images['img-toll-bridge'].get('credit') == 'A. N. Other'
    assert images['img-lantern'].get('credit') is None


def test_fill_in_the_cover(ui):
    ui.select('cover')
    f = ui.field
    f('Title').fill('Grand Forks')
    f('Subtitle').fill('A Town in Three Pieces')

    # Revert throws the draft away.
    expect(ui.viewer.locator('.dirty-note')).to_have_text('Unapplied changes')
    ui.viewer.get_by_role('button', name='Revert').click()
    expect(f('Title')).to_have_value('')
    f('Title').fill('Grand Forks')
    f('Subtitle').fill('A Town in Three Pieces')

    ui.check('Print cover art behind the text')
    f('Image').select_option('img-cover')
    f('Margin').fill('0.5in')
    f('Chaser').fill('A settlement on the Copperway')
    f('Subchaser').fill('Placeholder edition')
    f('Author').fill('The Grand Forks Test Suite')
    f('Colour').fill('white')
    ui.check('Drop shadow')
    f('Shadow colour').fill('black')
    ui.check('DMs Guild logo')
    ui.check('Enigma Narrative logo')
    f('Placed').select_option('above')
    ui.check('Enigma Narrative logo on the contents page')

    # The rough preview shows the art and finds both logo files.
    preview = ui.viewer.locator('.cover-page')
    expect(preview.locator('img.art')).to_be_visible()
    expect(preview.locator('.logo-stack img.logo')).to_have_count(2)
    expect(preview.locator('.logo.missing')).to_have_count(0)
    expect(preview).to_contain_text('Written by The Grand Forks Test Suite')

    ui.apply()
    expect(ui.viewer.locator('.dirty-note')).to_have_count(0)

    root = ui.xml(SUPP)
    cover = root.find('cover')
    assert (cover.get('color'), cover.get('shadow'), cover.get('shadowColor')) == ('white', 'true', 'black')
    assert cover.findtext('headerTitle') == 'Grand Forks'
    assert cover.findtext('headerSubtitle') == 'A Town in Three Pieces'
    art = cover.find('coverImage')
    assert (art.get('ref'), art.get('margin')) == ('img-cover', '0.5in')
    assert cover.findtext('footerChaser') == 'A settlement on the Copperway'
    assert cover.findtext('footerSubchaser') == 'Placeholder edition'
    assert cover.findtext('author') == 'The Grand Forks Test Suite'
    assert cover.find('dmsguildLogo') is not None
    assert cover.find('enigmaLogo').get('place') == 'above'
    assert root.find('contents/enigmaLogo') is not None
