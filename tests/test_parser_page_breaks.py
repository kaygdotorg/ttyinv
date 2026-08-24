from pathlib import Path

from ttyinv.parser import _page_break_indexes, _strip_directive_markers, _summary_only_indexes, parse_invoice_file


def _invoice_source(section_count: int, marked: set[int]) -> str:
    sections: list[str] = []
    for index in range(section_count):
        marker = "<!-- ttyinv:page-break-before -->\n" if index in marked else ""
        sections.append(
            f"{marker}## Repeated title\n\n"
            "| Description | Quantity | Rate | Amount (USD) |\n"
            "| --- | ---: | ---: | ---: |\n"
            f"| Fabricated row {index} | 1 | 1.00 | 1.00 |"
        )
    return (
        "---\n"
        "schema: ttyinv/v1\n"
        "invoice:\n"
        "  number: INV-FABRICATED-PAGE-BREAKS\n"
        "  issued: 2026-08-01\n"
        "  currency: USD\n"
        "from:\n"
        "  name: Example Seller\n"
        "to:\n"
        "  name: Example Buyer\n"
        "---\n\n"
        + "\n\n".join(sections)
        + "\n"
    )


def test_page_break_marker_is_bound_to_immediate_heading_with_duplicate_titles(tmp_path: Path) -> None:
    source = _invoice_source(4, {1, 3})
    path = tmp_path / "duplicate-titles.md"
    path.write_text(source, encoding="utf-8")

    parsed = parse_invoice_file(path)

    assert [section.page_break_before for section in parsed.sections] == [False, True, False, True]


def test_page_break_mapping_scales_with_high_cardinality_fabricated_input() -> None:
    section_count = 10_000
    marked = set(range(0, section_count, 3))
    body = "\n".join(
        f"<!-- ttyinv:page-break-before -->\n## Duplicate title {index}"
        if index in marked
        else f"## Duplicate title {index}"
        for index in range(section_count)
    )

    assert _page_break_indexes(body) == marked


def test_blank_line_prevents_marker_from_binding_to_a_later_heading() -> None:
    body = "<!-- ttyinv:page-break-before -->\n\n## Later heading\n"

    assert _page_break_indexes(body) == set()


def test_marker_and_heading_like_text_inside_fenced_code_are_not_directives() -> None:
    body = "```markdown\n<!-- ttyinv:page-break-before -->\n## Not a heading\n```\n## Actual heading\n"

    assert _page_break_indexes(body) == set()


def test_summary_only_marker_is_positional_and_safe_inside_fenced_code() -> None:
    body = "```markdown\n<!-- ttyinv:summary-only -->\n## Not a heading\n```\n<!-- ttyinv:summary-only -->\n## Actual heading\n"

    assert _summary_only_indexes(body) == {0}
    stripped = _strip_directive_markers(body)
    assert "```markdown\n<!-- ttyinv:summary-only -->" in stripped
    assert "<!-- ttyinv:summary-only -->\n## Actual heading" not in stripped
