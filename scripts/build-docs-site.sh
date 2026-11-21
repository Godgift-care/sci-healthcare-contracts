#!/usr/bin/env bash
#
# Builds the documentation site: docs/ from the current checkout at the site
# root, and a frozen copy of docs/ from every release tag under /<tag>/.
#
# Usage:  ./scripts/build-docs-site.sh [output-dir]       (default: _site)
#
# Environment:
#   DOCS_BASE_PATH  the path the site is served under, without a trailing
#                   slash: "/sci-healthcare-contracts" on <owner>.github.io,
#                   "" on a custom domain. Defaults to "/<repository name>".
#   DOCS_TAGS       space-separated tags to snapshot instead of every release
#                   tag (vMAJOR.MINOR.PATCH). Set it to "" to build main only.
#
# Requires: git with the tags fetched (`git fetch --tags`), Ruby and the
# gems in scripts/docs-site/Gemfile (`BUNDLE_GEMFILE=scripts/docs-site/Gemfile
# bundle install`).
#
# Layout of the output:
#
#   /                 docs/ from the checkout (main when CI publishes)
#   /v0.2.0/          docs/ as it was at the tag v0.2.0
#   /latest/          redirects to the newest release
#
# Every build is told which version it is, which release is the newest and
# where the site root is, through a generated config file. The banner in
# docs/_includes/header_custom.html reads those values, and is copied into
# each snapshot so tags cut before it existed get the banner too. A tag whose
# docs/ has no _config.yml (v0.1.0) is built with the current one.
#
# What breaks if this is wrong: the site root is replaced on every publish,
# so a bug here can take the whole docs site down, or label the docs for
# main as a release that integrators then build against.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-_site}"
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac

die() { printf 'error: %s\n' "$1" >&2; exit 1; }
say() { printf '\n==> %s\n' "$1"; }

if [ -z "${DOCS_BASE_PATH+set}" ]; then
  DOCS_BASE_PATH="/$(basename "$(git -C "$ROOT" rev-parse --show-toplevel)")"
fi
BASE="${DOCS_BASE_PATH%/}"
case "$BASE" in ''|/*) ;; *) die "DOCS_BASE_PATH must start with / (got '$DOCS_BASE_PATH')" ;; esac

export BUNDLE_GEMFILE="${BUNDLE_GEMFILE:-$ROOT/scripts/docs-site/Gemfile}"
command -v bundle >/dev/null || die "bundler not found. Install Ruby, then: BUNDLE_GEMFILE=scripts/docs-site/Gemfile bundle install"

# Release tags, newest first. Pre-release tags (v1.0.0-rc.1) are left out:
# they are neither the latest release nor something to integrate against.
if [ -n "${DOCS_TAGS+set}" ]; then
  TAGS="$DOCS_TAGS"
else
  TAGS="$(git -C "$ROOT" tag --list 'v*' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -t. -k1.2,1nr -k2,2nr -k3,3nr | tr '\n' ' ' || true)"
fi
LATEST="${TAGS%% *}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# build <source dir> <destination dir> <version label> <baseurl>
build() {
  local src="$1" dest="$2" version="$3" baseurl="$4"
  local extra="$WORK/version-$version.yml"
  # The plugins and markdown settings GitHub's legacy Pages builder applied
  # on its own (see scripts/docs-site/Gemfile), then the version values.
  cat > "$extra" <<YAML
plugins:
  - jekyll-remote-theme
  - jekyll-relative-links
  - jekyll-optional-front-matter
  - jekyll-titles-from-headings
  - jekyll-default-layout
  - jekyll-readme-index
  - jekyll-seo-tag
  - jekyll-include-cache
sass:
  quiet_deps: true
markdown: kramdown
kramdown:
  input: GFM
  hard_wrap: false
docs_version: "$version"
docs_latest: "$LATEST"
docs_root: "$BASE"
YAML
  # --baseurl wins over anything the snapshot's own config says, so links in
  # a snapshot always stay inside that snapshot.
  bundle exec jekyll build --quiet \
    --source "$src" \
    --destination "$dest" \
    --config "$src/_config.yml,$extra" \
    --baseurl "$baseurl"
}

say "Building docs/ from the checkout at ${BASE:-/}/"
cp -R "$ROOT/docs" "$WORK/main"
# The root is built first: jekyll empties its destination, so building it
# after the snapshots would delete them.
build "$WORK/main" "$OUT" "main" "$BASE"

for tag in $TAGS; do
  say "Building docs/ at $tag into ${BASE}/$tag/"
  if ! git -C "$ROOT" cat-file -e "$tag:docs" 2>/dev/null; then
    printf '  %s has no docs/ directory; skipped\n' "$tag"
    continue
  fi
  snap="$WORK/snap-$tag"
  mkdir -p "$snap"
  git -C "$ROOT" archive "$tag" docs | tar -x -C "$snap"
  src="$snap/docs"
  if [ ! -f "$src/_config.yml" ]; then
    printf '  %s has no docs/_config.yml; using the current one\n' "$tag"
    cp "$ROOT/docs/_config.yml" "$src/_config.yml"
  fi
  mkdir -p "$src/_includes"
  cp "$ROOT/docs/_includes/header_custom.html" "$src/_includes/header_custom.html"
  build "$src" "$OUT/$tag" "$tag" "$BASE/$tag"
done

if [ -n "$LATEST" ] && [ -d "$OUT/$LATEST" ]; then
  say "Pointing ${BASE}/latest/ at $LATEST"
  mkdir -p "$OUT/latest"
  cat > "$OUT/latest/index.html" <<HTML
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Latest release: $LATEST</title>
<meta http-equiv="refresh" content="0; url=$BASE/$LATEST/">
<link rel="canonical" href="$BASE/$LATEST/">
</head>
<body>
<p>The latest release is <a href="$BASE/$LATEST/">$LATEST</a>.</p>
</body>
</html>
HTML
fi

say "Site written to $OUT"
printf '  main       %s/\n' "${BASE}"
for tag in $TAGS; do
  [ -d "$OUT/$tag" ] && printf '  %-10s %s/%s/\n' "$tag" "$BASE" "$tag"
done
[ -n "$LATEST" ] && printf '  latest     %s/latest/ -> %s\n' "$BASE" "$LATEST"
exit 0
