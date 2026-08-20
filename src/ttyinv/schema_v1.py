"""Public JSON Schema for ttyinv/v1 frontmatter."""

from __future__ import annotations

import json
from copy import deepcopy
from pathlib import Path

_SCHEMA: dict[str, object] = {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://github.com/kaygdotorg/ttyinv/blob/main/schema/ttyinv-v1.schema.json",
    "title": "ttyinv/v1 invoice frontmatter",
    "type": "object",
    "additionalProperties": False,
    "required": ["schema", "invoice", "from", "to"],
    "properties": {
        "schema": {"const": "ttyinv/v1"},
        "invoice": {
            "type": "object",
            "additionalProperties": True,
            "required": ["number", "issued", "currency"],
            "properties": {
                "number": {"type": "string", "minLength": 1},
                "title": {"type": "string", "default": "Invoice"},
                "issued": {"type": "string", "format": "date"},
                "due": {"type": "string", "format": "date"},
                "currency": {"type": "string", "pattern": "^[A-Z]{3}$"},
                "locale": {"type": "string", "default": "en-GB"},
                "reference": {"type": "string"},
                "terms": {"type": "string"},
            },
        },
        "from": {"$ref": "#/$defs/party"},
        "to": {"$ref": "#/$defs/party"},
        "payment": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "title": {"type": "string", "default": "Payment"},
                "methods": {"type": "array", "items": {"$ref": "#/$defs/paymentMethod"}},
            },
        },
        "settlements": {"type": "array", "items": {"type": "object", "additionalProperties": True}},
        "signature": {
            "type": "object",
            "additionalProperties": False,
            "required": ["image"],
            "properties": {
                "image": {"type": "string"}, "name": {"type": "string"},
                "label": {"type": "string", "default": "Authorized signature"}, "alt": {"type": "string"},
            },
        },
        "appearance": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "theme": {"enum": ["light", "dark"]}, "font": {"type": "string"},
                "accent": {"type": "string"}, "paper": {"type": "string"},
                "ink": {"type": "string"}, "muted": {"type": "string"},
                "density": {"enum": ["comfortable", "compact"]},
            },
        },
    },
    "$defs": {
        "party": {
            "type": "object",
            "additionalProperties": False,
            "required": ["name"],
            "properties": {
                "name": {"type": "string", "minLength": 1},
                "address": {"type": "array", "items": {"type": "string"}},
                "identifiers": {"type": "object", "additionalProperties": {"type": "string"}},
                "email": {"type": "string", "format": "email"},
                "website": {"type": "string", "format": "uri"},
                "logo": {"type": "string"}, "logo_alt": {"type": "string"},
            },
        },
        "paymentMethod": {
            "type": "object",
            "additionalProperties": False,
            "required": ["title", "fields"],
            "properties": {
                "title": {"type": "string"},
                "fields": {"type": "object", "additionalProperties": {"type": "string"}},
            },
        },
    },
    "x-ttyinv-markdown": {
        "sectionHeading": "Level-two headings (##) define sections.",
        "financialTable": "A GFM table with an Amount or Amount (CUR) column is financial.",
        "calculation": "Blank or auto Amount values are calculated from Quantity/Qty/Days/Hours × Rate/Unit price.",
        "rawHtml": "Raw HTML is excluded except the literal <br> separator inside table cells.",
    },
}


def schema() -> dict[str, object]:
    return deepcopy(_SCHEMA)


def schema_json() -> str:
    return json.dumps(schema(), indent=2, sort_keys=True) + "\n"


def write_schema(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(schema_json(), encoding="utf-8")
