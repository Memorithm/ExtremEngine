#!/usr/bin/env bash
set -euo pipefail

readonly advisory_id="RUSTSEC-2026-0192"
readonly expected_issue="https://github.com/Memorithm/ExtremEngine/issues/46"
readonly expected_owner="ExtremEngine maintainers"

reason_line="$(grep -F "$advisory_id" deny.toml)"
expiry="$(printf '%s\n' "$reason_line" | sed -n 's/.*Valid through: \([0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]\).*/\1/p')"
review_date="${ADVISORY_REVIEW_DATE:-$(date -u +%F)}"

if [[ -z "$expiry" || "$reason_line" != *"Owner: $expected_owner"* || "$reason_line" != *"Tracking: $expected_issue"* ]]; then
  echo "::error::${advisory_id} must carry one owner, tracking issue and ISO expiry in deny.toml"
  exit 1
fi
if [[ "$review_date" > "$expiry" ]]; then
  echo "::error::${advisory_id} waiver expired after ${expiry}; remove it or document a newly reviewed bounded waiver"
  exit 1
fi
if grep -R --include=Cargo.toml -nE '(^|[[:space:]])ttf-parser[[:space:]]*=' Cargo.toml crates; then
  echo "::error::${advisory_id} scope changed: ttf-parser is now a direct workspace dependency"
  exit 1
fi

tree="$(cargo tree -i ttf-parser --locked --edges normal)"
printf '%s\n' "$tree"
for expected in "ttf-parser" "ab_glyph" "sctk-adwaita" "winit" "extrem_window"; do
  if ! grep -Fq "$expected" <<<"$tree"; then
    echo "::error::${advisory_id} dependency-path evidence no longer contains ${expected}"
    exit 1
  fi
done

echo "${advisory_id} waiver valid through ${expiry}; dependency path remains decoration-only"
