from __future__ import annotations

import json
from pathlib import Path

from ttyinv.cli_v2 import main
from ttyinv.schema_v1 import schema


def test_schema_is_versioned_and_requires_core_fields() -> None:
    document = schema()
    assert document["properties"]["schema"]["const"] == "ttyinv/v1"
    assert set(document["required"]) == {"schema", "invoice", "from", "to"}


def test_schema_command_writes_json(tmp_path: Path) -> None:
    output = tmp_path / "ttyinv-v1.schema.json"
    assert main(["schema", "--output", str(output)]) == 0
    parsed = json.loads(output.read_text(encoding="utf-8"))
    assert parsed["title"].startswith("ttyinv/v1")


def test_init_writes_only_fabricated_starter_data(tmp_path: Path) -> None:
    invoice = tmp_path / "invoice.md"
    assert main(["init", str(invoice)]) == 0
    content = invoice.read_text(encoding="utf-8")
    assert "Northstar Studio" in content
    assert "example.com" in content
    assert "ttyinv/v1" in content


def test_init_refuses_to_overwrite_without_force(tmp_path: Path) -> None:
    invoice = tmp_path / "invoice.md"
    invoice.write_text("keep me", encoding="utf-8")
    assert main(["init", str(invoice)]) == 1
    assert invoice.read_text(encoding="utf-8") == "keep me"
