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
            "additionalProperties": False,
            "required": ["number", "issued", "currency"],
            "properties": {
                "number": {"type": "string", "minLength": 1},
                "kind": {"enum": ["standard", "gst"], "default": "standard"},
                "title": {"type": "string", "default": "Invoice"},
                "issued": {"type": "string"},
                "due": {"type": "string"},
                "currency": {"type": "string", "pattern": "^[A-Za-z]{3}$"},
                "locale": {"type": "string", "default": "en-GB"},
                "terms": {"type": "string"},
            },
        },
        "from": {"$ref": "#/$defs/party"},
        "to": {"$ref": "#/$defs/party"},
        "payment": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "title": {"type": "string", "default": "Payment Methods"},
                "methods": {"type": "array", "items": {"$ref": "#/$defs/paymentMethod"}},
                "pageBreakBefore": {"type": "boolean", "default": False},
            },
        },
        "settlements": {
            "type": "array",
            "items": {"$ref": "#/$defs/settlement"},
        },
        "signature": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "image": {"type": "string"},
                "name": {"type": "string"},
                "label": {"type": "string"},
            },
        },
        "appearance": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "accent": {"type": "string"},
                "paper": {"type": "string"},
                "ink": {"type": "string"},
                "muted": {"type": "string"},
                "rule": {"type": "string"},
                "density": {"enum": ["comfortable", "compact"]},
                "font": {"$ref": "#/$defs/font"},
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
                "address": {"type": "array", "items": {"type": ["string", "number"]}},
                "identifiers": {"type": "object", "additionalProperties": {"type": ["string", "number"]}},
                "email": {"type": "string"},
                "website": {"type": "string"},
                "logo": {"type": "string"},
            },
        },
        "paymentMethod": {
            "type": "object",
            "additionalProperties": False,
            "required": ["title", "fields"],
            "properties": {
                "title": {"type": "string"},
                "fields": {"type": "object", "additionalProperties": {"type": ["string", "number"]}},
            },
        },
        "money": {
            "type": "object",
            "additionalProperties": False,
            "required": ["amount", "currency"],
            "properties": {
                "amount": {"type": ["string", "number"]},
                "currency": {"type": "string", "pattern": "^[A-Za-z]{3}$"},
            },
        },
        "settlement": {
            "type": "object",
            "additionalProperties": False,
            "required": ["date", "paid"],
            "properties": {
                "date": {"type": "string"},
                "paid": {"$ref": "#/$defs/money"},
                "received": {"$ref": "#/$defs/money"},
            },
        },
        "font": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "family": {"type": "string"},
                "regular": {"type": "string"},
                "bold": {"type": "string"},
            },
        },
    },
    "x-ttyinv-markdown": {
        "sectionHeading": "Level-two headings (##) define sections.",
        "financialTable": "A GFM table with an Amount or Amount (CUR) column is financial.",
        "calculation": "Blank or auto Amount values are calculated from Quantity/Qty/Days/Hours × Rate/Unit price.",
        "rawHtml": "Raw HTML is excluded except the literal <br> separator inside table cells.",
        "pageBreak": "The exact <!-- ttyinv:page-break-before --> marker immediately before an H2 requests a page break; structured callers may set payment.pageBreakBefore for the payment section.",
        "summaryOnly": "The exact <!-- ttyinv:summary-only --> marker immediately before an H2 marks a recap table whose rows are displayed but excluded from invoice total arithmetic.",
        "summaryRows": "Description values Subtotal, Total, and Grand Total preserve authored numeric payable amounts without contributing to generated totals.",
        "invoiceKinds": "standard renders the invoice total in words; gst additionally renders the received settlement amount in words.",
    },
}


def schema() -> dict[str, object]:
    return deepcopy(_SCHEMA)


def schema_json() -> str:
    return json.dumps(schema(), indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def write_schema(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(schema_json(), encoding="utf-8")
