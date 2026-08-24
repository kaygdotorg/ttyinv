#!/usr/bin/env python3
"""Assert ttyinv's relational geometry without storing a private reference image.

The check tests relationships rather than fragile screenshot bytes: the literal
corner glyphs are centered on the single dashed page frame, section labels are
centered on their section rules, table columns share a right edge with totals,
and the page keeps A4 shape. An optional screenshot/report can still be uploaded
by CI for human review.
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


def bottom(value: dict[str, float]) -> float:
    return value["y"] + value["height"]


def require(condition: bool, message: str, failures: list[str]) -> None:
    if not condition:
        failures.append(message)


def inspect(page: Page) -> dict[str, Any]:
    failures: list[str] = []
    metrics: dict[str, Any] = {}

    sheet = box(page, ".invoice-sheet")
    require(sheet is not None, "expected one invoice sheet", failures)
    if sheet:
        a4_page_height = sheet["width"] * 297 / 210
        page_count = max(1, math.ceil((sheet["height"] - TOLERANCE_PX) / a4_page_height))
        metrics["page"] = {
            "width": sheet["width"],
            "content_height": sheet["height"],
            "a4_page_height": a4_page_height,
            "estimated_page_count": page_count,
        }
        require(
            sheet["height"] + TOLERANCE_PX >= a4_page_height,
            f"invoice sheet height {sheet['height']:.2f}px is shorter than A4",
            failures,
        )

    frame = box(page, ".page-frame")
    corners = {
        "top-left": box(page, ".frame-corner.tl"),
        "top-right": box(page, ".frame-corner.tr"),
        "bottom-right": box(page, ".frame-corner.br"),
        "bottom-left": box(page, ".frame-corner.bl"),
    }
    metrics["frame"] = frame
    metrics["frame_corners"] = corners
    require(frame is not None, "expected one outer page frame", failures)
    require(all(corners.values()), "expected four typographic frame corners", failures)

    if frame and all(corners.values()):
        tl, tr, br, bl = corners["top-left"], corners["top-right"], corners["bottom-right"], corners["bottom-left"]
        assert tl and tr and br and bl
        require(close(center_x(tl), frame["x"]) and close(center_y(tl), frame["y"]), "top-left + is not centered on the frame intersection", failures)
        require(close(center_x(tr), right(frame)) and close(center_y(tr), frame["y"]), "top-right + is not centered on the frame intersection", failures)
        require(close(center_x(bl), frame["x"]) and close(center_y(bl), bottom(frame)), "bottom-left + is not centered on the frame intersection", failures)
        require(close(center_x(br), right(frame)) and close(center_y(br), bottom(frame)), "bottom-right + is not centered on the frame intersection", failures)

    label_reports: list[dict[str, Any]] = []
    label_locators = page.locator("[data-ttyinv-section-label]").all()
    for label in label_locators:
        label_box = label.bounding_box()
        parent_box = label.locator("xpath=..").bounding_box()
        if label_box and parent_box:
            label_reports.append({"label": label_box, "section": parent_box})
            require(close(center_y(label_box), parent_box["y"]), "section label is not vertically centered on its ASCII rule", failures)
    metrics["section_labels"] = label_reports
    if len(label_reports) > 1:
        origin = label_reports[0]["label"]["x"]
        require(all(close(item["label"]["x"], origin) for item in label_reports[1:]), "section labels do not share one left inset", failures)

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
