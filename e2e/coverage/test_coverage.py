"""The coverage check, after the story and the secondary suite: every edit the backend
takes, every section block kind, and every UI feature in features.toml must have been used.

- Edits: each variant of each *Edit enum in shared/src (read from the source, so the list
  can't fall behind the code), plus the Cover, Batch and Pills changes, against the
  change log's Saved applies.
- Section blocks: each SECTION_BLOCKS kind, in the supplement the story wrote.
- UI features: each [features] entry needs a test of its own that passed in this run
  (story or edges).

Only a whole run counts (every story step, and edges/): with --from-snapshot, --attach or
a -k that leaves some out, the log has only part of it, so this is skipped."""
import json
import re
import tomllib
from pathlib import Path

import pytest
from lxml import etree

import harness

SHARED = harness.PROTO_UI / 'shared' / 'src'
# Change targets and the enum each one's `edit` is.
TARGETS = {'Chapter': 'ChapterEdit', 'Reference': 'RefEdit', 'Block': 'BlockEdit', 'Table': 'TableEdit', 'List': 'ListEdit',
           'Box': 'BoxEdit', 'Monster': 'MonsterEdit', 'Family': 'FamilyEdit', 'Stat': 'StatEdit'}
WHOLE = ['Cover', 'Element', 'Batch', 'Pills']        # changes with no `op` to check


def enum_variants():
    """{enum name: [variant, ...]} for every `pub enum …Edit` in shared/src."""
    out = {}
    for f in SHARED.glob('*.rs'):
        text = f.read_text(encoding='utf-8')
        for m in re.finditer(r'pub enum (\w+Edit)\s*\{', text):
            depth, j = 1, m.end()
            while depth:
                depth += {'{': 1, '}': -1}.get(text[j], 0)
                j += 1
            names, d = [], 0
            for line in text[m.end():j - 1].splitlines():
                s = line.strip()
                if d == 0 and not s.startswith(('//', '#')):
                    v = re.match(r'^([A-Z]\w*)\s*(\{|\(|,|$)', s)
                    if v:
                        names.append(v.group(1))
                d += line.count('{') - line.count('}')
            out[m.group(1)] = names
    return out


def section_blocks():
    text = (SHARED / 'block.rs').read_text(encoding='utf-8')
    body = re.search(r'pub const SECTION_BLOCKS: &\[&str\] = &\[(.*?)\];', text, re.S).group(1)
    return re.findall(r'"(\w+)"', body)


def used(changes):
    seen = set()

    def walk(c):
        seen.add(c['target'])
        for sub in c.get('changes', []):
            walk(sub)
        e = c.get('edit')
        if isinstance(e, dict) and 'op' in e:
            seen.add(f"{c['target']}.{e['op']}")
    for c in changes:
        if c['result']['status'] == 'Saved':
            walk(c['change'])
    return seen


@pytest.fixture(autouse=True)
def whole_story(pytestconfig, request):
    if pytestconfig.getoption('--from-snapshot') or pytestconfig.getoption('--attach'):
        pytest.skip('coverage needs the whole story in one run')
    steps = {i.path.name for i in request.session.items if re.match(r'test_(0[1-9]|1[0-2])_', i.path.name)}
    if len(steps) < 12:
        pytest.skip('coverage needs every story step (this run selected some)')
    if not any('edges' in i.path.parts for i in request.session.items):
        pytest.skip('coverage needs the secondary suite (edges/) in the run too')


def test_every_edit_was_applied(app):
    seen = used(app.changes())
    variants = enum_variants()
    assert set(TARGETS.values()) <= set(variants), f'an *Edit enum moved or was renamed: found {sorted(variants)}'
    want = [f'{t}.{v}' for t, e in TARGETS.items() for v in variants[e]] + WHOLE
    missing = [w for w in want if w not in seen]
    assert not missing, 'never applied (add a test that uses them): ' + ', '.join(missing)


def test_every_section_block_kind(app):
    # The story's own last snapshot: edges/ has changed the project since.
    root = etree.parse(str(app.run_dir / 'snapshots' / '12' / 'grand-forks' / 'supplement.xml')).getroot()
    missing = [k for k in section_blocks() if not root.xpath(f'//section/{k}')]
    assert not missing, f'section blocks the supplement never got: {missing}'


def test_every_ui_feature(request):
    spec = tomllib.loads((harness.E2E / 'features.toml').read_text(encoding='utf-8'))
    collected = {f'{i.path.name}::{i.originalname}' for i in request.session.items}
    unknown, uncovered = [], []
    for feature, tests in spec['features'].items():
        for t in tests:
            if t not in collected:
                unknown.append(f'{feature}: {t}')
        if not any(t in harness.PASSED for t in tests):
            uncovered.append(feature)
    assert not unknown, 'features.toml names tests that don\'t exist:\n' + '\n'.join(unknown)
    assert not uncovered, 'no passing test for: ' + ', '.join(uncovered)
    pending = spec.get('pending', {})
    if pending:
        print('\nUI features with no test yet:\n' + '\n'.join(f'  {f} ({why})' for f, why in pending.items()))
