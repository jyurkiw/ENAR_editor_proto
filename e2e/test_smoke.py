"""The harness works: the e2e build launches, Playwright attaches, the seams answer, and a
project opens through the real Open project… button and Project settings dialog."""
import tomllib

from playwright.sync_api import expect

from art.make_slop_art import ART, main as make_art


def test_seams_are_installed(app):
    assert app.page.evaluate('typeof window.__enigmaE2E.confirm') == 'function'
    app.answer_confirms(False)
    assert app.page.evaluate('() => window.__enigmaE2E.confirm("Delete it?")') is False
    assert app.page.evaluate('() => window.__enigmaE2E.confirm("And this?")') is True   # OK once answers run out
    assert app.confirms_asked() == ['Delete it?', 'And this?']


def test_open_and_configure_project(app, page):
    expect(page.get_by_text('No project is open.')).to_be_visible()
    app.queue_pick(app.project_dir)
    page.get_by_role('complementary').get_by_role('button', name='Open project…').click()   # the file panel's, not the header's

    dialog = page.locator('.modal')
    expect(dialog.get_by_role('heading', name='Project settings')).to_be_visible()
    schemas, compiler = dialog.locator('input[type=text]').all()
    assert schemas.input_value().replace('\\', '/').endswith('schemas')          # found by walking up
    assert compiler.input_value().replace('\\', '/').endswith('compiler')
    dialog.get_by_role('button', name='Save').click()
    expect(dialog).to_have_count(0)

    root = page.locator('.tree [data-path]').first
    expect(root).to_contain_text('grand-forks')

    cfg = tomllib.loads((app.project_dir / 'enigma-project.toml').read_text(encoding='utf-8'))
    assert (app.project_dir / cfg['schemas_dir'] / 'supplement-template-schema.xsd').is_file()
    assert (app.project_dir / cfg['compiler_dir'] / 'pyproject.toml').is_file()


def test_slop_art(tmp_path):
    from PIL import Image
    paths = make_art(tmp_path)
    assert [p.name for p in paths] == list(ART)
    for p in paths:
        with Image.open(p) as img:
            assert img.size == ART[p.name][0]
