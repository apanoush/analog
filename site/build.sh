#!/usr/bin/env bash
# Build the static site:
#   1. render README.md into the landing page (site/index.html), and
#   2. build the mdbook documentation into site/docs/.
#
# The repository URL shown in the header (and in the book menu) defaults to a
# placeholder; override it with REPO_URL, e.g.
#   REPO_URL=https://github.com/you/analog bash site/build.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

repo_url="${REPO_URL:-https://github.com/OWNER/analog}"

echo "==> rendering README.md -> site/index.html"
pandoc README.md \
    --from gfm --to html5 --standalone \
    --template site/template.html \
    --metadata title="analog" \
    --metadata repo_url="$repo_url" \
    --metadata docs_url="./docs/index.html" \
    --syntax-highlighting=tango \
    --output site/index.html

echo "==> building docs -> site/docs"
mdbook build docs -d "$root/site/docs"

echo "==> done"
echo "    preview the landing page: python3 -m http.server -d site"
echo "    preview the docs live:    mdbook serve docs"
