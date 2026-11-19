"""Step 01: open an empty folder as a project, through Open project… and Project settings,
and look around the file tree."""
import tomllib

from playwright.sync_api import expect


def test_cancelled_picker_opens_nothing(ui, app):
    expect(ui.page.get_by_text('No project is open.')).to_be_visible()
    app.queue_pick(None)
    ui.page.get_by_role('complementary').get_by_role('button', name='Open project…').click()
    expect(ui.page.locator('.toolbar .project')).to_have_text('no project')
    expect(ui.modal).to_have_count(0)


def test_configure_the_project(ui, app):
    app.queue_pick(app.project_dir)
    ui.page.get_by_role('complementary').get_by_role('button', name='Open project…').click()

    expect(ui.modal.get_by_role('heading', name='Project settings')).to_be_visible()
    schemas, compiler = ui.dialog_field('Schemas folder'), ui.dialog_field('Compiler folder')
    assert schemas.input_value().replace('\\', '/').endswith('/schemas')        # found by walking up
    assert compiler.input_value().replace('\\', '/').endswith('/compiler')

    # A wrong folder is refused, and the dialog stays.
    good = schemas.input_value()
    schemas.fill(str(app.project_dir))
    ui.modal.get_by_role('button', name='Save').click()
    expect(ui.modal.locator('.problem')).to_be_visible()

    # Browse… fills the field from the picker.
    app.queue_pick(good)
    ui.modal.locator('label.field', has_text='Schemas folder').get_by_role('button', name='Browse…').click()
    expect(schemas).to_have_value(good)
    ui.modal.get_by_role('button', name='Save').click()
    expect(ui.modal).to_have_count(0)
    expect(ui.page.locator('.toolbar .project')).to_have_text('grand-forks')

    cfg = tomllib.loads((app.project_dir / 'enigma-project.toml').read_text(encoding='utf-8'))
    assert (app.project_dir / cfg['schemas_dir'] / 'supplement-template-schema.xsd').is_file()
    assert (app.project_dir / cfg['compiler_dir'] / 'pyproject.toml').is_file()


def test_file_tree(ui):
    ui.expand()
    root = ui.file_row()
    expect(root).to_contain_text('grand-forks')
    toml = ui.file_row('enigma-project.toml')
    expect(toml).to_be_visible()
    expect(toml).to_have_class('row file other')

    # Only .xml files can be opened.
    toml.click(button='right')
    opener = ui.page.locator('.menu').get_by_role('button', name='Open')
    expect(opener).to_be_disabled()
    expect(opener).to_have_attribute('title', 'Only .xml files can be opened')
    ui.page.locator('.menu-backdrop').click()
    expect(ui.page.locator('.menu')).to_have_count(0)

    # A folder's menu: the New… items and Refresh.
    (ui.root / 'scratch.txt').write_text('made behind the editor\'s back')
    expect(ui.file_row('scratch.txt')).to_have_count(0)
    ui.file_menu('', 'Refresh')
    expect(ui.file_row('scratch.txt')).to_be_visible()
    (ui.root / 'scratch.txt').unlink()
    ui.file_menu('', 'Refresh')
    expect(ui.file_row('scratch.txt')).to_have_count(0)

    # Clicking the root closes and opens it.
    root.click()
    expect(toml).to_have_count(0)
    root.click()
    expect(toml).to_be_visible()
