#!/usr/bin/env bash
# Build the static site:
#   1. fetch the latest GitHub release and render the downloads page,
#   2. render README.md into the landing page (site/index.html), and
#   3. build the mdbook documentation into site/docs/.
#
# Requires: pandoc, mdbook, curl, python3.
#
# The repository URL defaults to the upstream project; override it with
# REPO_URL, e.g. REPO_URL=https://github.com/you/analog bash site/build.sh
# The releases API slug is derived from REPO_URL (override with REPO_SLUG).
# Set GITHUB_TOKEN to lift the API rate limit in CI.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

repo_url="${REPO_URL:-https://github.com/apanoush/analog}"
repo_slug="${REPO_SLUG:-${repo_url#https://github.com/}}"
repo_slug="${repo_slug%.git}"

echo "==> fetching latest release for $repo_slug"
release_json="$(mktemp)"
trap 'rm -f "$release_json"' EXIT

curl_args=(-sSfL --proto '=https' --tlsv1.2
           -H "Accept: application/vnd.github+json")
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl_args+=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
fi
if ! curl "${curl_args[@]}" \
        "https://api.github.com/repos/${repo_slug}/releases/latest" \
        -o "$release_json"; then
    echo "error: could not fetch the latest release for ${repo_slug}." >&2
    echo "       refusing to fall back to a stale version; check the network" >&2
    echo "       or GITHUB_TOKEN and retry." >&2
    exit 1
fi

echo "==> rendering downloads -> site/downloads.html"
python3 site/gen_downloads.py "$release_json" "$repo_url" site/downloads.html

echo "==> rendering README.md -> site/index.html"
pandoc README.md \
    --from gfm --to html5 --standalone \
    --template site/template.html \
    --lua-filter site/rewrite-links.lua \
    --metadata title="analog" \
    --metadata repo_url="$repo_url" \
    --metadata docs_url="./docs/index.html" \
    --metadata downloads_url="./downloads.html" \
    --syntax-highlighting=tango \
    --output site/index.html

echo "==> building docs -> site/docs"
# mdbook never deletes files that disappeared from the sources, so clear the
# output directory to avoid shipping removed pages (e.g. old decision records).
rm -rf site/docs
mdbook build docs -d "$root/site/docs"

# Stop GitHub Pages from running Jekyll on the generated tree.
: > site/.nojekyll

echo "==> done"
echo "    preview the landing page: python3 -m http.server -d site"
echo "    preview the docs live:    mdbook serve docs"
