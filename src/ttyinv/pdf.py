from __future__ import annotations

import os
import platform
import tempfile
from pathlib import Path

from playwright.sync_api import sync_playwright

from .errors import TtyinvError


def find_chromium(explicit: str | None = None) -> Path:
    candidates: list[str | None] = [
        explicit,
        os.environ.get("PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH"),
        os.environ.get("CHROME_PATH"),
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ]
    local_app_data = os.environ.get("LOCALAPPDATA")
    program_files = os.environ.get("PROGRAMFILES")
    if local_app_data:
        candidates.append(str(Path(local_app_data) / "Google/Chrome/Application/chrome.exe"))
    if program_files:
        candidates.append(str(Path(program_files) / "Google/Chrome/Application/chrome.exe"))

    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    raise TtyinvError(
        "No Chromium-based browser found. Install Chromium/Google Chrome, pass --chromium PATH, "
        "or set PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH."
    )


def render_pdf(html_document: str, output_path: Path, chromium_path: str | None = None) -> None:
    executable = find_chromium(chromium_path)
    with tempfile.TemporaryDirectory(prefix="ttyinv-") as temporary_directory:
        html_path = Path(temporary_directory) / "invoice.html"
        html_path.write_text(html_document, encoding="utf-8")
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(
                executable_path=str(executable),
                headless=True,
                args=["--allow-file-access-from-files"],
            )
            try:
                page = browser.new_page(viewport={"width": 1120, "height": 1584})
                page.set_content(html_document, wait_until="networkidle")
                page.emulate_media(media="print")
                page.evaluate(
                    """async () => {
                        await document.fonts.ready;
                        await Promise.all(Array.from(document.images).map((image) => {
                          if (image.complete) return Promise.resolve();
                          return new Promise((resolve) => {
                            image.addEventListener('load', resolve, {once: true});
                            image.addEventListener('error', resolve, {once: true});
                          });
                        }));
                    }"""
                )
                page.pdf(
                    path=str(output_path),
                    format="A4",
                    print_background=True,
                    prefer_css_page_size=True,
                    margin={"top": "0", "right": "0", "bottom": "0", "left": "0"},
                )
            finally:
                browser.close()
