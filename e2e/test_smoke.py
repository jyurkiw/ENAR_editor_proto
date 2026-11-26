"""The harness works: the e2e build launches, Playwright attaches and the seams answer.
(Opening a project is story step 01.)"""
from art.make_slop_art import ART, main as make_art


def test_seams_are_installed(app):
    assert app.page.evaluate('typeof window.__enigmaE2E.confirm') == 'function'
    app.confirms_asked()                        # whatever earlier tests left
    app.answer_confirms(False)
    assert app.page.evaluate('() => window.__enigmaE2E.confirm("Delete it?")') is False
    assert app.page.evaluate('() => window.__enigmaE2E.confirm("And this?")') is True   # OK once answers run out
    assert app.confirms_asked() == ['Delete it?', 'And this?']


def test_slop_art(tmp_path):
    from PIL import Image
    paths = make_art(tmp_path)
    assert [p.name for p in paths] == list(ART)
    for p in paths:
        with Image.open(p) as img:
            assert img.size == ART[p.name][0]
