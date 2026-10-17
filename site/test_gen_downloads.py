"""Tests for the downloads page generator (site/gen_downloads.py)."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import gen_downloads


def make_release() -> dict:
    base = "https://github.com/acme/analog/releases/download/v9.9.9/"
    names = [
        "analog-cli-x86_64-pc-windows-msvc.msi",
        "analog-cli-x86_64-pc-windows-msvc.msi.sha256",
        "analog-cli-aarch64-apple-darwin.tar.xz",
        "analog-cli-aarch64-apple-darwin.tar.xz.sha256",
        "analog-gui-x86_64-unknown-linux-gnu.tar.xz",
        "analog-gui-x86_64-unknown-linux-gnu.tar.xz.sha256",
        "source.tar.gz",
        "dist-manifest.json",
    ]
    return {
        "tag_name": "v9.9.9",
        "html_url": "https://github.com/acme/analog/releases/tag/v9.9.9",
        "published_at": "2030-01-02T03:04:05Z",
        "assets": [
            {"name": n, "browser_download_url": base + n, "size": 1024 * 1024}
            for n in names
        ],
    }


def test_parse_asset_matches_known_packages() -> None:
    assert gen_downloads.parse_asset("analog-cli-x86_64-pc-windows-msvc.msi") == (
        "analog-cli",
        "x86_64-pc-windows-msvc",
        ".msi",
    )
    assert gen_downloads.parse_asset("analog-gui-aarch64-apple-darwin.tar.xz") == (
        "analog-gui",
        "aarch64-apple-darwin",
        ".tar.xz",
    )
    assert gen_downloads.parse_asset("sha256.sum") is None
    assert (
        gen_downloads.parse_asset("analog-cli-x86_64-pc-windows-msvc.msi.sha256")
        is None
    )


@pytest.mark.parametrize(
    ("size", "expected"),
    [(512, "512 B"), (1024, "1.0 KB"), (1024 * 1024, "1.0 MB")],
)
def test_human_size(size: int, expected: str) -> None:
    assert gen_downloads.human_size(size) == expected


def test_main_pairs_checksums_and_skips_internal_files(tmp_path: Path) -> None:
    release_file = tmp_path / "release.json"
    release_file.write_text(json.dumps(make_release()), encoding="utf-8")
    out = tmp_path / "downloads.html"

    assert (
        gen_downloads.main(
            [
                "gen_downloads.py",
                str(release_file),
                "https://github.com/acme/analog",
                str(out),
            ]
        )
        == 0
    )
    page = out.read_text(encoding="utf-8")

    assert "<strong>v9.9.9</strong>" in page
    assert "analog-cli-x86_64-pc-windows-msvc.msi" in page
    # A checksum link is emitted for every payload that has a sibling .sha256.
    assert page.count(">sha256</a>") == 3
    # Internal build files must never be advertised.
    assert "dist-manifest.json" not in page
    assert "source.tar.gz" in page
