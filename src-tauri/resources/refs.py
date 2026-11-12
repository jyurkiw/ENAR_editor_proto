"""Resolve the <reference> entries of one supplement file to the plain text the
compiler would print for each <monsterRef>/<nounRef>/<spellRef>/<itemRef>/<dataRef>
value, and print the result as JSON.

Run by the Enigma Narrative UI through the compiler's uv project, with the compiler's
folder on the path, so files load exactly as the compiler loads them:

    uv run --project <compiler> python refs.py <file.xml> <compiler dir>

Output (stdout, one JSON object):
    {"ok": bool,
     "entries": {id: {"kind": str, "values": {value: text}, "errors": {value: message}}},
     "problems": [str]}
The text mirrors encompile.text.ref_text without the LaTeX: the same fallbacks
(shortName -> displayName -> name), the same files, the same nesting (a spell whose
<name> holds a <monsterRef> from its own file).
"""
import json
import os
import sys

VALUES = {
    'monster': ('name', 'displayName', 'shortName', 'cr', 'src'),
    'noun': ('name', 'displayName', 'shortName'),
    'spell': ('name', 'src'),
    'item': ('name', 'displayName', 'src'),
    'data': ('value', 'body'),
}
REF_TAGS = ('monsterRef', 'nounRef', 'spellRef', 'itemRef', 'dataRef')


class Unresolved(Exception):
    pass


def plain(el, doc, depth=0):
    """Mixed content -> plain text, resolving typed refs against `doc`'s entries."""
    out = [el.text or '']
    for c in el:
        if isinstance(c.tag, str):
            if c.tag in REF_TAGS:
                e = doc.refs.get(c.get('ref'))
                if e is None:
                    raise Unresolved(f'<{c.tag} ref="{c.get("ref")}"> has no <reference> entry in {doc.rel()}')
                out.append(text_of(e, c.get('value'), depth + 1))
            elif c.tag == 'condition':
                out.append((c.text or '').capitalize())
            else:
                out.append(plain(c, doc, depth))
        out.append(c.tail or '')
    return ''.join(out)


def text_of(e, value, depth=0):
    if depth > 20:
        raise Unresolved('refs nest too deeply')
    if e.kind == 'data':
        return e.body if value == 'body' else e.get('value', '')
    if e.get(value):
        return e.get(value)
    if value in ('displayName', 'shortName'):
        return text_of(e, 'displayName' if value == 'shortName' and e.get('displayName') else 'name', depth)
    t = e.target
    if t is None:
        raise Unresolved(f'no {value} and no file to read it from')
    if e.kind == 'monster':
        m = e.monster()
        if m is None:
            raise Unresolved(f'no <monster id="{e.get("monster")}"> in {t.rel()}')
        if value == 'name':
            return m.findtext('name') or ''
        if value == 'src':
            return m.get('src') or 'Enigma Narrative'
        if value == 'cr':
            cr = m.find('attributes/challengeRatings/challengeRating')
            if cr is None:
                raise Unresolved(f'no challenge rating in {t.rel()}')
            return cr.get('value')
    if value == 'name':
        n = t.root.find('name')
        if n is None:
            raise Unresolved(f'no <name> in {t.rel()}')
        return plain(n, t, depth)
    if value == 'src':
        return t.root.get('src') or 'Enigma Narrative'
    raise Unresolved(f"can't print {value}")


def main():
    if len(sys.argv) != 3:
        print(json.dumps({"ok": False, "entries": {}, "problems": ["usage: refs.py FILE.xml COMPILER_DIR"]}))
        return
    path, compiler = os.path.abspath(sys.argv[1]), sys.argv[2]
    sys.path.insert(0, compiler)
    try:
        from encompile.loader import Library, CompileError
    except Exception as e:  # noqa: BLE001 - anything that stops the import
        print(json.dumps({"ok": False, "entries": {}, "problems": [f"can't load the compiler: {e}"]}))
        return
    lib = Library(os.path.dirname(path), validate=False)
    try:
        doc = lib._load(os.path.normpath(path), ())
    except CompileError as e:
        print(json.dumps({"ok": False, "entries": {}, "problems": [str(e)]}))
        return
    entries = {}
    for eid, e in doc.refs.items():
        if e.kind not in VALUES:
            continue
        vals, errs = {}, {}
        for v in VALUES[e.kind]:
            try:
                vals[v] = text_of(e, v)
            except (Unresolved, CompileError) as x:
                errs[v] = str(x)
            except Exception as x:  # noqa: BLE001 - a broken file must not stop the rest
                errs[v] = f'{type(x).__name__}: {x}'
        entries[eid] = {"kind": e.kind, "values": vals, "errors": errs}
    print(json.dumps({"ok": True, "entries": entries, "problems": list(lib.problems)}))


if __name__ == "__main__":
    main()
