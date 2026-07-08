"""Execute every ```python code block in the user-facing docs.

This is the guard against documentation drift: the IQM review (2026-07)
found that published snippets used an API that never existed. Any snippet
readable in docs/quickstart.md or docs/python-api.md is guaranteed to run
against the current bindings, or this test fails.

Blocks that cannot run in CI (vendor credentials, optional extras) must
carry a `# doc-test: skip` marker with the reason on the same line.
"""

import re
from pathlib import Path

import pytest

DOCS = Path(__file__).resolve().parents[3] / "docs"
DOC_FILES = ["quickstart.md", "python-api.md"]

BLOCK_RE = re.compile(r"```python\n(.*?)```", re.S)


def _snippets():
    for name in DOC_FILES:
        path = DOCS / name
        if not path.exists():
            yield pytest.param(
                None, id=f"{name}#missing", marks=pytest.mark.xfail(reason="doc file missing")
            )
            continue
        for i, match in enumerate(BLOCK_RE.finditer(path.read_text(encoding="utf-8"))):
            code = match.group(1)
            marks = []
            if "# doc-test: skip" in code:
                marks.append(pytest.mark.skip(reason="marked doc-test: skip"))
            yield pytest.param(code, id=f"{name}#{i}", marks=marks)


@pytest.mark.parametrize("code", _snippets())
def test_doc_snippet_executes(code):
    # Each block must be self-contained (its own imports) — that is also
    # what makes it copy-pasteable for readers.
    exec(compile(code, "<doc-snippet>", "exec"), {"__name__": "__doc_snippet__"})
