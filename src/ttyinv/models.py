from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date
from decimal import Decimal
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from .colors import validate_css_color
from .dates import canonical_date
from .errors import TtyinvError


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


class FontConfig(StrictModel):
    family: str | None = None
    regular: str | None = None
    bold: str | None = None


Density = Literal["comfortable", "compact"]


class AppearanceConfig(StrictModel):
    accent: str | None = None
    paper: str | None = None
    ink: str | None = None
    muted: str | None = None
    rule: str | None = None
    density: Density | None = None
    font: FontConfig | None = None

    @field_validator("accent", "paper", "ink", "muted", "rule")
    @classmethod
    def validate_color(cls, value: str | None) -> str | None:
        if value is None:
            return None
        try:
            return validate_css_color(value)
        except TtyinvError as exc:
            raise ValueError(str(exc)) from exc


class Party(StrictModel):
    name: str
    address: list[str] = Field(default_factory=list)
    identifiers: dict[str, str] = Field(default_factory=dict)
    email: str | None = None
    website: str | None = None
    logo: str | None = None

    @field_validator("address", mode="before")
    @classmethod
    def normalise_address(cls, value: object) -> object:
        if value is None:
            return []
        if isinstance(value, str):
            return [line.strip() for line in value.splitlines() if line.strip()]
        if isinstance(value, (list, tuple)):
            return [str(line).strip() for line in value if str(line).strip()]
        return value

    @field_validator("identifiers", mode="before")
    @classmethod
    def stringify_identifiers(cls, value: object) -> object:
        if value is None:
            return {}
        if isinstance(value, dict):
            return {str(key): str(item) for key, item in value.items()}
        return value


class InvoiceMeta(StrictModel):
    number: str
    kind: Literal["standard", "gst"] = "standard"
    title: str = "Invoice"
    issued: str
    due: str | None = None
    terms: str | None = None
    currency: str
    locale: str = "en-GB"

    @field_validator("issued", "due", mode="before")
    @classmethod
    def normalise_date_scalar(cls, value: object) -> object:
        if value is None:
            return value
        return canonical_date(value)

    @model_validator(mode="after")
    def validate_date_order(self) -> InvoiceMeta:
        if not self.due:
            return self
        issued = date.fromisoformat(self.issued)
        due = date.fromisoformat(self.due)
        if due < issued:
            raise ValueError("due date must be on or after issue date")
        return self


    @field_validator("currency")
    @classmethod
    def normalise_currency(cls, value: str) -> str:
        value = value.upper()
        if len(value) != 3 or not value.isalpha():
            raise ValueError("currency must be a three-letter ISO-style code")
        return value


class PaymentMethod(StrictModel):
    title: str
    fields: dict[str, str] = Field(default_factory=dict)

    @field_validator("fields", mode="before")
    @classmethod
    def stringify_fields(cls, value: object) -> object:
        if value is None:
            return {}
        if isinstance(value, dict):
            return {str(key): str(item) for key, item in value.items()}
        return value


class PaymentConfig(StrictModel):
    title: str = "Payment Methods"
    methods: list[PaymentMethod] = Field(default_factory=list)
    # Keep the public frontmatter spelling aligned with the structured REST /
    # MCP contract. ``populate_by_name`` still lets Python callers construct
    # this model with the internal snake_case name.
    page_break_before: bool = Field(default=False, alias="pageBreakBefore")


class SignatureConfig(StrictModel):
    image: str | None = None
    name: str | None = None
    label: str | None = None


class MoneyValue(StrictModel):
    amount: Decimal
    currency: str

    @field_validator("currency")
    @classmethod
    def normalise_currency(cls, value: str) -> str:
        value = value.upper()
        if len(value) != 3 or not value.isalpha():
            raise ValueError("currency must be a three-letter ISO-style code")
        return value


class Settlement(StrictModel):
    date: str
    paid: MoneyValue
    received: MoneyValue | None = None

    @field_validator("date", mode="before")
    @classmethod
    def normalise_date_scalar(cls, value: object) -> object:
        return canonical_date(value)


class InvoiceFrontmatter(StrictModel):
    schema_version: Literal["ttyinv/v1"] = Field(alias="schema")
    invoice: InvoiceMeta
    issuer: Party = Field(alias="from")
    recipient: Party = Field(alias="to")
    payment: PaymentConfig | None = None
    signature: SignatureConfig | None = None
    settlements: list[Settlement] = Field(default_factory=list)
    appearance: AppearanceConfig | None = None


Alignment = Literal["left", "right", "center"] | None


@dataclass(slots=True)
class TableCell:
    source: str
    html: str
    line: int | None = None
    column: int | None = None


@dataclass(slots=True)
class ParsedTable:
    headers: list[TableCell]
    align: list[Alignment]
    rows: list[list[TableCell]]
    row_lines: list[int | None] = field(default_factory=list)


@dataclass(slots=True)
class FinancialSection:
    title: str
    table: ParsedTable
    line: int | None = None
    page_break_before: bool = False
    summary_only: bool = False
    kind: Literal["financial"] = "financial"


@dataclass(slots=True)
class ProseSection:
    title: str
    html: str
    line: int | None = None
    page_break_before: bool = False
    summary_only: bool = False
    kind: Literal["prose"] = "prose"


InvoiceSection = FinancialSection | ProseSection


@dataclass(slots=True)
class ParsedInvoice:
    source_path: Path
    source_directory: Path
    frontmatter: InvoiceFrontmatter
    preamble_html: str
    sections: list[InvoiceSection]


@dataclass(slots=True)
class CalculatedCell:
    html: str
    plain: str
    numeric: bool = False


AmountSource = Literal["calculated", "explicit", "explicit-rounded-rate", "trusted-explicit", "authored-summary"]


@dataclass(slots=True)
class CalculatedRow:
    cells: list[CalculatedCell]
    amount: Decimal
    amount_source: AmountSource
    source_line: int | None = None
    summary_label: str | None = None


@dataclass(slots=True)
class CalculatedFinancialSection:
    title: str
    headers: list[str]
    align: list[Alignment]
    rows: list[CalculatedRow]
    total: Decimal
    payable_amount_column: int
    source_line: int | None = None
    warnings: list[str] = field(default_factory=list)
    page_break_before: bool = False
    summary_only: bool = False
    kind: Literal["financial"] = "financial"


@dataclass(slots=True)
class CalculatedProseSection:
    title: str
    html: str
    source_line: int | None = None
    page_break_before: bool = False
    summary_only: bool = False
    kind: Literal["prose"] = "prose"


CalculatedSection = CalculatedFinancialSection | CalculatedProseSection


@dataclass(slots=True)
class CalculatedInvoice:
    source_path: Path
    source_directory: Path
    frontmatter: InvoiceFrontmatter
    preamble_html: str
    sections: list[CalculatedSection]
    grand_total: Decimal
    warnings: list[str] = field(default_factory=list)


@dataclass(slots=True)
class AmountPolicy:
    trust_explicit: bool = False
    recalculate: bool = False


@dataclass(slots=True)
class RenderOptions:
    theme: Literal["light", "dark"]
    output_path: Path
    for_pdf: bool = False
    accent_override: str | None = None
    paper_override: str | None = None
    ink_override: str | None = None
    muted_override: str | None = None
    rule_override: str | None = None
    density_override: Density | None = None
    font_family_override: str | None = None
    allow_outside_root: bool = False
    deterministic: bool = False


@dataclass(slots=True)
class RenderResult:
    html: str
    warnings: list[str] = field(default_factory=list)
