"""Step 02: make the supplement and the files it uses with the New dialogs, from the file
tree's menu and the toolbar, and draw the placeholder art.

    grand-forks/                 (the project)
      grand-forks/supplement.xml   the settlement
      grand-forks/art/*.png        SLOP ART
      grand-forks/forkjaw-pike.xml a monster family
      grand-forks/ferrymans-lantern.xml
      townsfolk.xml                plain stat blocks (toolbar: the project root)
      fork-the-current.xml         a spell (toolbar)
"""
from playwright.sync_api import expect

from art.make_slop_art import ART, main as make_art

SUPP = 'grand-forks/supplement.xml'


def created(ui, rel):
    ui.idle()
    ui.no_error()
    expect(ui.modal).to_have_count(0)
    expect(ui.tab(rel)).to_be_visible()
    assert (ui.root / rel).is_file()


def test_new_supplement(opened):
    ui = opened
    ui.file_menu('', 'New supplement here…')
    expect(ui.modal.get_by_role('heading', name='New supplement')).to_be_visible()
    ui.dialog_field('Folder name').fill('grand-forks')
    expect(ui.dialog_field('Title')).to_have_value('Grand Forks')        # follows the folder name
    ui.dialog_field('Type').select_option('settlement')
    expect(ui.modal.locator('.preview')).to_contain_text('grand-forks\\grand-forks\\supplement.xml')
    ui.modal.get_by_role('button', name='Create').click()
    created(ui, SUPP)

    root = ui.xml(SUPP)
    assert root.get('type') == 'settlement'
    assert root.findtext('cover/headerTitle') == 'Grand Forks'

    # The same folder again is refused.
    ui.file_menu('', 'New supplement here…')
    ui.dialog_field('Folder name').fill('grand-forks')
    ui.modal.get_by_role('button', name='Create').click()
    expect(ui.modal.locator('.problem')).to_contain_text('already exists')
    ui.modal.get_by_role('button', name='Cancel').click()
    expect(ui.modal).to_have_count(0)


def test_art(ui):
    paths = make_art(ui.root / 'grand-forks' / 'art')
    assert len(paths) == len(ART)


def test_new_monster_file_from_the_tree(ui):
    ui.file_menu('grand-forks', 'New monster file here…')
    expect(ui.modal.get_by_role('heading', name='New monster file')).to_be_visible()
    ui.dialog_field('File name').fill('forkjaw-pike')
    expect(ui.dialog_field('Family name')).to_have_value('Forkjaw Pike')
    expect(ui.dialog_field('First stat block')).to_have_value('Forkjaw Pike')
    expect(ui.dialog_field('Its id')).to_have_value('forkjaw-pike')
    ui.dialog_field('Short noun').fill('pike')
    ui.modal.get_by_role('button', name='Create').click()
    created(ui, 'grand-forks/forkjaw-pike.xml')
    root = ui.xml('grand-forks/forkjaw-pike.xml')
    assert root.findtext('name') == 'Forkjaw Pike'
    assert root.findtext('shortNoun') == 'pike'
    assert root.find('monster').get('id') == 'forkjaw-pike'


def test_new_monster_file_from_the_toolbar(ui):
    ui.page.locator('.toolbar').get_by_role('button', name='New monster file…').click()
    ui.dialog_field('File name').fill('townsfolk')
    ui.dialog_field('Family name').fill('Townsfolk of Grand Forks')
    ui.dialog_field('Short noun').fill('townsperson')
    ui.dialog_field('First stat block').fill('Toll Warden')
    expect(ui.dialog_field('Its id')).to_have_value('toll-warden')
    ui.modal.get_by_role('button', name='Create').click()
    created(ui, 'townsfolk.xml')
    assert ui.xml('townsfolk.xml').find('monster').get('id') == 'toll-warden'


def test_new_magic_item(ui):
    ui.file_menu('grand-forks', 'New magic item here…')
    ui.dialog_field('File name').fill('ferrymans-lantern')
    expect(ui.dialog_field('Item name')).to_have_value('Ferrymans Lantern')
    ui.dialog_field('Item name').fill("Ferryman's Lantern")
    ui.dialog_field('Rarity').select_option('rare')
    ui.modal.get_by_role('button', name='Create').click()
    created(ui, 'grand-forks/ferrymans-lantern.xml')
    root = ui.xml('grand-forks/ferrymans-lantern.xml')
    assert root.get('id') == 'ferrymans-lantern'
    assert root.findtext('name') == "Ferryman's Lantern"
    assert root.findtext('rarities/rarity') == 'rare'


def test_new_spell(ui):
    ui.page.locator('.toolbar').get_by_role('button', name='New spell…').click()
    ui.dialog_field('File name').fill('fork-the-current')
    expect(ui.dialog_field('Spell name')).to_have_value('Fork The Current')
    ui.dialog_field('Spell name').fill('Fork the Current')
    ui.dialog_field('Level').select_option('2')
    ui.dialog_field('School').select_option('Transmutation')
    ui.modal.get_by_role('button', name='Create').click()
    created(ui, 'fork-the-current.xml')
    root = ui.xml('fork-the-current.xml')
    assert root.get('id') == 'fork-the-current'
    assert (root.findtext('name'), root.findtext('level'), root.findtext('school')) == ('Fork the Current', '2', 'Transmutation')


def test_tabs(ui):
    for rel in (SUPP, 'grand-forks/forkjaw-pike.xml', 'townsfolk.xml', 'grand-forks/ferrymans-lantern.xml', 'fork-the-current.xml'):
        expect(ui.tab(rel)).to_be_visible()
    # Closing a tab and opening the file again from the tree.
    ui.tab('fork-the-current.xml').locator('button.close').click()
    expect(ui.tab('fork-the-current.xml')).to_have_count(0)
    ui.open('fork-the-current.xml')
    ui.open(SUPP)

