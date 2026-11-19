"""Step 04: the chapters. Add chapter… for the three pieces of the town and the appendix,
the intro retitled, each chapter's sections laid out in its Contents (added, reordered,
one dropped before Apply), and a scratch chapter renamed and then deleted."""
from playwright.sync_api import expect

SUPP = 'grand-forks/supplement.xml'

# title, name (None: the slug the dialog makes), kind, sections in the order they end up in
CHAPTERS = [
    ('Tollside', None, 'chapter', [('toll-bridge', 'The Toll Bridge'), ('coaching-inn', 'The Copper Kettle Inn'),
                                   ('wardens-house', "The Warden's House")]),
    ('Millward', None, 'chapter', [('weir', 'The Weir'), ('mills', 'The Mills'), ('pike-attack', 'Trouble in the Water')]),
    ('The Wedge', 'wedge', 'chapter', [('market', 'Wedge Market'), ('ferry-shrine', 'The Ferry Shrine')]),
    ('Creatures of the Forks', 'creatures', 'appendix', [('creatures', 'Creatures')]),
]


def slug(title):
    words = [w.lower() for w in title.replace("'", ' ').split()]
    return '-'.join(words[1:] if len(words) > 1 and words[0] in ('the', 'a', 'an') else words)


def add_chapter(ui, title, name, kind):
    ui.select('supplement')
    ui.viewer.get_by_role('button', name='Add chapter…').click()
    expect(ui.modal.get_by_role('heading', name='Add chapter')).to_be_visible()
    ui.dialog_field('Title').fill(title)
    expect(ui.dialog_field('Name')).to_have_value(slug(title))
    if name:
        ui.dialog_field('Name').fill(name)
    ui.dialog_field('Kind').select_option(kind)
    ui.click_applies(ui.modal.get_by_role('button', name='Add chapter'))
    expect(ui.modal).to_have_count(0)


def chapter_names(ui):
    root = ui.xml(SUPP)
    return [c.get('name') for c in root.findall('chapter')], [c.get('name') for c in root.findall('chapterOrder/ch')]


def add_sections(ui, sections):
    row = ui.body.locator('.add-row')
    for key, title in sections:
        row.locator('select').select_option('section')
        row.locator('input').nth(0).fill(key)
        row.locator('input').nth(1).fill(title)
        row.get_by_role('button', name='Add').click()
        expect(row.locator('input').nth(0)).to_have_value('')


def section_rows(ui):
    return ui.body.locator('.child-row')


def test_add_chapters(opened):
    ui = opened
    ui.open(SUPP)
    for title, name, kind, _ in CHAPTERS:
        add_chapter(ui, title, name, kind)
    add_chapter(ui, 'Scratch Chapter', None, 'chapter')
    expected = ['introduction', 'tollside', 'millward', 'wedge', 'creatures', 'scratch-chapter']
    assert chapter_names(ui) == (expected, expected)
    assert ui.one(SUPP, '//chapter[@name="creatures"]').get('kind') == 'appendix'

    ui.select('supplement')
    expect(ui.viewer.locator('.notice.info', has_text='chapter(s).')).to_contain_text('6 chapter(s).')


def test_a_taken_name_is_refused(ui):
    ui.select('supplement')
    ui.viewer.get_by_role('button', name='Add chapter…').click()
    ui.dialog_field('Title').fill('Tollside')
    expect(ui.modal.locator('.problem')).to_have_text('There\'s already a chapter named "tollside"')
    expect(ui.modal.get_by_role('button', name='Add chapter')).to_be_disabled()
    ui.modal.get_by_role('button', name='Cancel').click()
    expect(ui.modal).to_have_count(0)


def test_retitle_the_introduction(ui):
    ui.select('chapter', name='introduction')
    ui.field('Title').fill('Where the River Splits')
    ui.field('Columns').select_option('1')
    ui.field('Credit').fill('Test Author')
    add_sections(ui, [('three-pieces', 'The Three Pieces')])
    expect(section_rows(ui).last).to_contain_text('new')
    ui.apply()

    ch = ui.one(SUPP, '//chapter[@name="introduction"]')
    assert ch.findtext('chapterTitle') == 'Where the River Splits'
    assert (ch.get('columns'), ch.get('credit')) == ('1', 'Test Author')
    assert [s.get('name') for s in ch.findall('section')] == ['intro-opening', 'three-pieces']


def test_lay_out_the_sections(ui):
    for title, name, _, sections in CHAPTERS:
        name = name or slug(title)
        ui.select('chapter', name=name)
        expect(ui.field('Title')).to_have_value(title)
        if name == 'tollside':
            # Added out of order and with a stray one: move the inn up, drop the stray.
            first, inn, house = sections
            add_sections(ui, [first, house, inn, ('stray', 'Stray')])
            rows = section_rows(ui)
            keys = rows.locator('.child-key')
            expect(keys).to_have_text(['toll-bridge', 'wardens-house', 'coaching-inn', 'stray'])
            rows.filter(has_text='Stray').get_by_title('Delete (on Apply)').click()
            expect(keys).to_have_text(['toll-bridge', 'wardens-house', 'coaching-inn'])
            rows.filter(has_text='The Copper Kettle Inn').get_by_title('Move up').click()
            expect(keys).to_have_text(['toll-bridge', 'coaching-inn', 'wardens-house'])
        else:
            add_sections(ui, sections)
        ui.apply()
        ch = ui.one(SUPP, f'//chapter[@name="{name}"]')
        assert [(s.get('name'), s.findtext('title')) for s in ch.findall('section')] == sections, name

    # The skeleton's empty paragraph is the only body any of them has so far.
    assert len(ui.xml(SUPP).findall('chapter/section')) == 2 + sum(len(c[3]) for c in CHAPTERS)


def test_rename_and_delete_the_scratch_chapter(ui, app):
    ui.select('chapter', name='scratch-chapter')
    ui.field('Name').fill('scratch-renamed')
    expect(ui.body.locator('.warn-text')).to_contain_text('renames the')
    ui.apply()
    names, order = chapter_names(ui)
    assert names[-1] == order[-1] == 'scratch-renamed'

    ui.select('chapter', name='scratch-renamed')
    delete = ui.body.get_by_role('button', name='Delete chapter')
    app.answer_confirms(False)
    delete.click()
    asked = app.confirms_asked()
    assert len(asked) == 1 and 'scratch-renamed' in asked[0], asked
    assert 'scratch-renamed' in chapter_names(ui)[0]

    app.answer_confirms(True)
    ui.click_applies(delete)
    assert app.confirms_asked() == asked
    expected = ['introduction', 'tollside', 'millward', 'wedge', 'creatures']
    assert chapter_names(ui) == (expected, expected)
