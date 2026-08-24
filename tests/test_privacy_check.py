import hashlib
import importlib.util
import shutil
import sys
from pathlib import Path


ROOT = Path(__file__).parents[1]
_PRIVACY_CHECK_PATH = ROOT / "scripts" / "privacy_check.py"
_PRIVACY_CHECK_SPEC = importlib.util.spec_from_file_location("ttyinv_privacy_check", _PRIVACY_CHECK_PATH)
if _PRIVACY_CHECK_SPEC is None or _PRIVACY_CHECK_SPEC.loader is None:
    raise ImportError(f"Unable to load {_PRIVACY_CHECK_PATH}")
_PRIVACY_CHECK = importlib.util.module_from_spec(_PRIVACY_CHECK_SPEC)
sys.modules[_PRIVACY_CHECK_SPEC.name] = _PRIVACY_CHECK
_PRIVACY_CHECK_SPEC.loader.exec_module(_PRIVACY_CHECK)

ALLOWED_BINARY_FILES = _PRIVACY_CHECK.ALLOWED_BINARY_FILES
main = _PRIVACY_CHECK.main


def test_reviewed_screenshots_match_pinned_hashes() -> None:
    for relative, expected_digest in ALLOWED_BINARY_FILES.items():
        actual_digest = hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()
        assert actual_digest == expected_digest, relative


def test_privacy_check_rejects_replaced_screenshot(tmp_path: Path, capsys) -> None:
    asset = Path("docs/screenshots/editor-desktop.jpg")
    destination = tmp_path / asset
    destination.parent.mkdir(parents=True)
    shutil.copyfile(ROOT / asset, destination)
    data = bytearray(destination.read_bytes())
    data[-1] ^= 0x01
    destination.write_bytes(data)

    assert main([str(tmp_path)]) == 1
    assert (
        f"unreviewed binary {asset}: SHA-256 does not match the reviewed asset"
        in capsys.readouterr().err
    )
