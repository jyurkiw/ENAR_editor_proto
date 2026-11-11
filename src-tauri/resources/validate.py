"""Validate one XML file against one XSD with lxml and print the result as JSON.

Run by the Enigma Narrative UI through the compiler's uv project, so it uses the same
lxml the compiler does:

    uv run --project <compiler> python validate.py <file.xml> <schema.xsd>

Output (stdout, one JSON object):
    {"ok": bool, "errors": [{"line": int, "column": int, "message": str}, ...]}
A file that isn't well-formed XML, or a schema that can't be loaded, is reported the
same way with ok=false.
"""
import json
import sys


def report(ok, errors):
    print(json.dumps({"ok": ok, "errors": errors}))


def log_errors(log):
    return [{"line": e.line, "column": e.column, "message": e.message} for e in log]


def main():
    if len(sys.argv) != 3:
        report(False, [{"line": 0, "column": 0, "message": "usage: validate.py FILE.xml SCHEMA.xsd"}])
        return
    path, xsd = sys.argv[1], sys.argv[2]
    try:
        from lxml import etree
    except ImportError:
        report(False, [{"line": 0, "column": 0, "message": "lxml is not installed in this Python environment"}])
        return
    try:
        schema = etree.XMLSchema(etree.parse(xsd))
    except (etree.XMLSyntaxError, etree.XMLSchemaParseError, OSError) as e:
        report(False, [{"line": 0, "column": 0, "message": f"can't load schema {xsd}: {e}"}])
        return
    try:
        doc = etree.parse(path)
    except etree.XMLSyntaxError as e:
        report(False, log_errors(e.error_log) or [{"line": 0, "column": 0, "message": str(e)}])
        return
    except OSError as e:
        report(False, [{"line": 0, "column": 0, "message": f"can't read {path}: {e}"}])
        return
    ok = schema.validate(doc)
    report(ok, [] if ok else log_errors(schema.error_log))


if __name__ == "__main__":
    main()
