#!/usr/bin/env python3
"""Render site/downloads.html from a GitHub release JSON payload.

The release metadata is fetched at build time by site/build.sh, so the page
always advertises the *latest* release and never hardcodes a version. Release
assets are grouped by application (CLI, GUI) and platform, with each archive
or installer paired with its ``.sha256`` checksum when one is published.

Usage:
    gen_downloads.py <release.json> <repo_url> <output.html>
"""

from __future__ import annotations

import html
import json
import sys
from pathlib import Path

# Applications built by dist, in display order: (asset prefix, title, blurb).
APPS: list[tuple[str, str, str]] = [
    ("analog-cli", "analog-cli", "command line tool"),
    ("analog-gui", "analog-gui", "desktop editor"),
]

# Target triple -> friendly platform label, in display order.
PLATFORMS: list[tuple[str, str]] = [
    ("x86_64-pc-windows-msvc", "Windows (x86_64)"),
    ("aarch64-apple-darwin", "macOS (Apple silicon)"),
    ("x86_64-apple-darwin", "macOS (Intel)"),
    ("x86_64-unknown-linux-gnu", "Linux (x86_64)"),
    ("aarch64-unknown-linux-gnu", "Linux (arm64)"),
]

# Archive/installer extensions, in display order, with a human label.
PACKAGE_KINDS: list[tuple[str, str]] = [
    (".msi", "installer"),
    (".zip", "portable"),
    (".tar.xz", "archive"),
    (".tar.gz", "archive"),
]


def parse_asset(name: str) -> tuple[str, str, str] | None:
    """Split an asset name into (app, target triple, extension)."""
    for app, _, _ in APPS:
        prefix = app + "-"
        if name.startswith(prefix):
            rest = name[len(prefix) :]
            break
    else:
        return None
    for triple, _ in PLATFORMS:
        for ext, _ in PACKAGE_KINDS:
            if rest == triple + ext:
                return app, triple, ext
    return None


def human_size(num_bytes: int) -> str:
    value = float(num_bytes)
    for unit in ("B", "KB", "MB", "GB"):
        if value < 1024 or unit == "GB":
            return f"{value:.0f} {unit}" if unit == "B" else f"{value:.1f} {unit}"
        value /= 1024
    return f"{value:.1f} GB"


def render(release: dict, repo_url: str) -> str:
    release_url = html.escape(release.get("html_url") or f"{repo_url}/releases")

    # Index assets so checksums can be paired with their payload.
    assets = {a["name"]: a for a in release.get("assets", [])}

    # app -> triple -> list[(ext, asset)]
    grouped: dict[str, dict[str, list[tuple[str, dict]]]] = {
        app: {} for app, _, _ in APPS
    }
    extras: list[dict] = []
    for name, asset in assets.items():
        parsed = parse_asset(name)
        if parsed is None:
            if name.endswith(".sha256") or name in ("dist-manifest.json", "sha256.sum"):
                continue
            extras.append(asset)
            continue
        app, triple, ext = parsed
        grouped[app].setdefault(triple, []).append((ext, asset))

    parts: list[str] = []
    for app, title, blurb in APPS:
        platforms = grouped[app]
        if not platforms:
            continue
        parts.append('<div class="download-card">')
        parts.append(
            f"  <h3>{html.escape(title)}"
            f' <span class="tag">{html.escape(blurb)}</span></h3>'
        )
        parts.append("  <table>")
        parts.append(
            "    <thead><tr><th>Platform</th><th>Package</th>"
            "<th>Size</th><th>Checksum</th></tr></thead>"
        )
        parts.append("    <tbody>")
        for triple, _label in PLATFORMS:
            if triple not in platforms:
                continue
            label = dict(PLATFORMS)[triple]
            present = {ext: asset for ext, asset in platforms[triple]}
            for ext, kind in PACKAGE_KINDS:
                asset = present.get(ext)
                if asset is None:
                    continue
                url = html.escape(asset["browser_download_url"])
                # Windows publishes both an installer and a portable zip, so
                # name the platform once and label each package.
                package = (
                    f'<a href="{url}">{html.escape(asset["name"])}</a>'
                    f' <span class="muted">{kind}</span>'
                )
                checksum_url = url + ".sha256"
                checksum = (
                    f'<a href="{checksum_url}">sha256</a>'
                    if asset["name"] + ".sha256" in assets
                    else '<span class="muted">n/a</span>'
                )
                parts.append(
                    f"      <tr><td>{html.escape(label)}</td>"
                    f"<td>{package}</td>"
                    f"<td>{human_size(asset.get('size', 0))}</td>"
                    f"<td>{checksum}</td></tr>"
                )
        parts.append("    </tbody>")
        parts.append("  </table>")
        parts.append("</div>")

    # Source tarball / other release attachments, if any.
    source_bits = []
    for asset in sorted(extras, key=lambda a: a["name"]):
        url = html.escape(asset["browser_download_url"])
        source_bits.append(f'<li><a href="{url}">{html.escape(asset["name"])}</a></li>')
    if source_bits:
        parts.append("<h2>Source and other files</h2>")
        parts.append('<ul class="download-extras">' + "".join(source_bits) + "</ul>")

    if not parts:
        parts.append(
            "<p>No binary packages are attached to this release yet. "
            f'See the <a href="{release_url}">release page</a>.</p>'
        )

    return "\n".join(parts)


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    release_path, repo_url, output_path = argv[1], argv[2], argv[3]

    release = json.loads(Path(release_path).read_text(encoding="utf-8"))
    template = (
        Path(__file__).with_name("downloads.template.html").read_text(encoding="utf-8")
    )

    published = (release.get("published_at") or "")[:10]
    page = (
        template.replace("{{REPO_URL}}", html.escape(repo_url))
        .replace("{{RELEASE_TAG}}", html.escape(release.get("tag_name") or "latest"))
        .replace("{{RELEASE_URL}}", html.escape(release.get("html_url") or repo_url))
        .replace("{{RELEASE_DATE}}", html.escape(published or "recently"))
        .replace("{{DOWNLOADS}}", render(release, repo_url))
    )
    Path(output_path).write_text(page, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
