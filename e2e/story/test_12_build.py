"""Step 12: the round trip. Every file the story wrote is compared with its golden copy in
e2e/golden/grand-forks/ (indentation ignored), then the compiler builds the supplement to a
PDF in the run folder: the PDF, the datasheet and the cover picture must all be there, the
datasheet must list the uncredited lantern picture, and the compiler may warn only about
what's expected.

    uv run pytest story --update-golden      rewrites golden/ from this run (review the diff)
"""
import difflib
import re
import shutil

import pytest
from lxml import etree

import harness

GOLDEN = harness.E2E / 'golden' / 'grand-forks'
# Text files compared as they are; XML files compared after re-indenting.
WORDS = 'enigma-words.txt'

# Warnings the build is allowed, and why.
EXPECTED = [
    # The compiler records ISBNs but doesn't print them yet.
    re.compile(r'<isbn value="[^"]*"> is recorded but not printed'),
]


def story_files(root):
    return sorted(p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file() and (p.suffix == '.xml' or p.name == WORDS))


def normal(path):
    if path.suffix != '.xml':
        return path.read_text(encoding='utf-8')
    tree = etree.parse(str(path), etree.XMLParser(remove_blank_text=True, remove_comments=False))
    return etree.tostring(tree, pretty_print=True, encoding='unicode')


def test_the_files_match_golden(app, pytestconfig):
    root = app.project_dir
    files = story_files(root)
    if pytestconfig.getoption('--update-golden'):
        if GOLDEN.exists():
            shutil.rmtree(GOLDEN)
        for rel in files:
            dest = GOLDEN / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(normal(root / rel), encoding='utf-8', newline='\n')
        pytest.skip(f'golden/ rewritten from this run ({len(files)} files): review the diff')
    if not GOLDEN.exists():
        pytest.fail('no golden files yet: run `uv run pytest story --update-golden` once and commit e2e/golden/')
    assert files == story_files(GOLDEN), 'the story wrote a different set of files than golden/ has'
    diffs = []
    for rel in files:
        got, want = normal(root / rel), (GOLDEN / rel).read_text(encoding='utf-8')
        if got != want:
            diffs.append(''.join(difflib.unified_diff(want.splitlines(True), got.splitlines(True), f'golden/{rel}', f'run/{rel}', n=2)))
    assert not diffs, 'the editor wrote something different (run with --update-golden if that\'s intended):\n' + '\n'.join(diffs)[:6000]


def latex_errors(log):
    return re.findall(r'^! (.*)$', log.read_text(encoding='utf-8', errors='replace'), flags=re.M) if log.exists() else []


def test_the_compiler_builds_it(app):
    out = app.run_dir / 'build'
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    pdf = out / 'grand_forks.pdf'
    code, text = harness.compile_supplement(app.data_repo, app.project_dir / 'grand-forks' / 'supplement.xml', pdf)
    (out / 'compiler.txt').write_text(text, encoding='utf-8')
    assert code == 0, text
    assert pdf.is_file() and pdf.stat().st_size > 10_000, text
    assert (out / 'grand_forks_cover.png').is_file(), text
    sheet = (out / 'grand_forks_datasheet.txt').read_text(encoding='utf-8')
    assert 'Title: Grand Forks' in sheet
    checks = sheet.split('Images to check', 1)[1].split('\n\n', 1)[0]
    assert "Ferryman's Lantern" in checks, sheet

    warnings = [l[len('warning: '):] for l in text.splitlines() if l.startswith('warning: ')]
    unexpected = []
    for w in warnings:
        if any(p.search(w) for p in EXPECTED):
            continue
        if w.startswith('pdflatex reported'):
            # The Windows copy of the dnd class fails on DndReadAloud and DndSidebar with no
            # options ("I do not know the key '/tcb/\NoValue'"); the Linux one doesn't. Any
            # other LaTeX error counts.
            errs = latex_errors(out / 'grand_forks.log')
            if errs and all("/tcb/\\NoValue" in e for e in errs):
                continue
        unexpected.append(w)
    assert not unexpected, 'unexpected compiler warnings:\n' + '\n'.join(unexpected)
