#!/usr/bin/env python3
"""Assert ttyinv's relational geometry without storing a private reference image.

The check deliberately tests relationships rather than fragile screenshot bytes:
outer-frame junctions share axes with their rules, table columns share a right
edge with totals, section labels share one inset, and the page keeps A4 shape.
An optional screenshot/report can still be uploaded by CI for human review.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

from playwright.sync_api import Browser, Page, sync_playwright

TOLERANCE_PX = 1.25


def close(actual: float, expected: float, tolerance: float = TOLERANCE_PX) -> bool:
    return math.isclose(actual, expected, abs_tol=tolerance)


def box(page: Page, selector: str) -> dict[str, float] | None:
    locator = page.locator(selector).first
    if not locator.count():
        return None
    return locator.bounding_box()


def boxes(page: Page, selector: str) -> list[dict[str, float]]:
    values: list[dict[str, float]] = []
    for locator in page.locator(selector).all():
        if bounding := locator.bounding_box():
            values.append(bounding)
    return values


def center_x(value: dict[str, float]) -> float:
    return value["x"] + value["width"] / 2


def center_y(value: dict[str, float]) -> float:
    return value["y"] + value["height"] / 2


def right(value: dict[str, float]) -> float:
    return value["x"] + value["width"]


def require(condition: bool, message: str, failures: list[str]) -> None:
    if not condition:
        failures.append(message)


def inspect(page: Page) -> dict[str, Any]:
    failures: list[str] = []
    metrics: dict[str, Any] = {}

    viewport = page.evaluate("({width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight})")
    ratio = viewport["width"] / viewport["height"]
    metrics["page"] = {**viewport, "ratio": ratio}
    require(abs(ratio - 210 / 297) < 0.025, f"page ratio {ratio:.5f} is not A4-like", failures)

    lines = {side: box(page, f'[data-ttyinv-frame-line="{side}"]') for side in ("top", "right", "bottom", "left")}
    corners = {name: box(page, f'[data-ttyinv-frame-corner="{name}"]') for name in ("top-left", "top-right", "bottom-right", "bottom-left")}
    metrics["frame_lines"] = lines
    metrics["frame_corners"] = corners
    require(all(lines.values()), "expected four outer frame lines", failures)
    require(all(corners.values()), "expected four outer frame junctions", failures)

    if all(lines.values()) and all(corners.values()):
        top, right_line, bottom, left_line = lines["top"], lines["right"], lines["bottom"], lines["left"]
        tl, tr, br, bl = corners["top-left"], corners["top-right"], corners["bottom-right"], corners["bottom-left"]
        assert top and right_line and bottom and left_line and tl and tr and br and bl
        require(close(center_y(tl), center_y(top)) and close(center_y(tr), center_y(top)), "top junctions do not share the top-rule axis", failures)
        require(close(center_y(bl), center_y(bottom)) and close(center_y(br), center_y(bottom)), "bottom junctions do not share the bottom-rule axis", failures)
        require(close(center_x(tl), center_x(left_line)) and close(center_x(bl), center_x(left_line)), "left junctions do not share the left-rule axis", failures)
        require(close(center_x(tr), center_x(right_line)) and close(center_x(br), center_x(right_line)), "right junctions do not share the right-rule axis", failures)

    label_boxes = boxes(page, "[data-ttyinv-section-label]")
    metrics["section_labels"] = label_boxes
    if len(label_boxes) > 1:
        origin = label_boxes[0]["x"]
        require(all(close(item["x"], origin) for item in label_boxes[1:]), "section labels do not share one left inset", failures)

    table_reports: list[dict[str, Any]] = []
    for table in page.locator("[data-ttyinv-table]").all():
        table_box = table.bounding_box()
        header_cells = [item.bounding_box() for item in table.locator("thead th").all()]
        header_cells = [item for item in header_cells if item]
        last_cells = [item.bounding_box() for item in table.locator("tbody tr td:last-child").all()]
        last_cells = [item for item in last_cells if item]
        report = {"table": table_box, "headers": header_cells, "last_cells": last_cells}
        table_reports.append(report)
        if table_box and header_cells:
            require(close(header_cells[0]["x"], table_box["x"]), "table header does not begin on the table grid", failures)
            require(close(right(header_cells[-1]), right(table_box)), "table header does not end on the table grid", failures)
        if table_box and last_cells:
            require(all(close(right(item), right(table_box)) for item in last_cells), "amount cells do not share the table right edge", failures)
    metrics["tables"] = table_reports

    total_boxes = boxes(page, "[data-ttyinv-total]")
    metrics["totals"] = total_boxes
    if table_reports and total_boxes:
        last_table = next((report["table"] for report in reversed(table_reports) if report["table"]), None)
        if last_table:
            candidates = [item for item in total_boxes if item["y"] >= last_table["y"]]
            if candidates:
                require(any(close(right(item), right(last_table), 2.0) for item in candidates), "grand-total geometry does not align with the final amount edge", failures)

    metrics["failures"] = failures
    return metrics


def open_page(browser: Browser, input_path: Path) -> Page:
    page = browser.new_page(viewport={"width": 1058, "height": 1497}, device_scale_factor=1)
    page.goto(input_path.resolve().as_uri(), wait_until="networkidle")
    page.evaluate("document.fonts.ready")
    return page


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("html", type=Path)
    parser.add_argument("--screenshot", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        try:
            page = open_page(browser, args.html)
            report = inspect(page)
            if args.screenshot:
                args.screenshot.parent.mkdir(parents=True, exist_ok=True)
                page.screenshot(path=str(args.screenshot), full_page=True)
        finally:
            browser.close()

    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")
    if report["failures"]:
        for failure in report["failures"]:
            print(f"visual-contract: {failure}")
        return 1
    print("visual-contract: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
