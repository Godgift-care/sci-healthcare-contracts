'use strict';

// Fails a pull request that changes the contracts without a CHANGELOG.md
// entry, so whoever integrates the next release is not surprised by a
// contract change nobody wrote down. Run by .github/workflows/changelog.yml.
//
// The rule, in order:
//   1. Nothing under contracts/ changed      -> pass (docs, CI, examples...)
//   2. CHANGELOG.md changed                  -> pass
//   3. The pull request has `skip-changelog` -> pass, reported as skipped
//   4. Otherwise                             -> fail, listing the files
//
// Usage: PR_LABELS='["area: docs"]' node changelog-check.cjs <changed-files.txt>
// where the file holds one changed path per line.

const SKIP_LABEL = 'skip-changelog';
const CHANGELOG = 'CHANGELOG.md';
const CONTRACTS_PREFIX = 'contracts/';

/**
 * Decides whether a pull request passes the changelog check.
 *
 * @param {{ files: string[], labels: string[] }} input changed paths and label names
 * @returns {{ ok: boolean, reason: string, contractFiles: string[] }}
 */
function checkChangelog({ files, labels }) {
  const contractFiles = files.filter((f) => f.startsWith(CONTRACTS_PREFIX));
  if (contractFiles.length === 0) return { ok: true, reason: 'no-contract-change', contractFiles };
  if (files.includes(CHANGELOG)) return { ok: true, reason: 'changelog-updated', contractFiles };
  if (labels.includes(SKIP_LABEL)) return { ok: true, reason: 'skipped-by-label', contractFiles };
  return { ok: false, reason: 'changelog-missing', contractFiles };
}

/** One path per line; blank lines and surrounding whitespace ignored. */
function parseFiles(text) {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
}

/** The PR_LABELS environment value: a JSON array of names, or empty. */
function parseLabels(json) {
  if (!json || !json.trim()) return [];
  const parsed = JSON.parse(json);
  if (!Array.isArray(parsed)) throw new Error('PR_LABELS must be a JSON array of label names');
  return parsed.map(String);
}

function message(result) {
  switch (result.reason) {
    case 'no-contract-change':
      return 'No file under contracts/ changed; no changelog entry needed.';
    case 'changelog-updated':
      return `contracts/ changed and ${CHANGELOG} was updated.`;
    case 'skipped-by-label':
      return `contracts/ changed without a ${CHANGELOG} entry; skipped by the \`${SKIP_LABEL}\` label.`;
    default:
      return [
        `This pull request changes the contracts but not ${CHANGELOG}.`,
        `Add an entry under "## Unreleased" describing the change for integrators.`,
        `If the change cannot affect anyone using the contracts (a comment, a test-only refactor),`,
        `a maintainer can add the \`${SKIP_LABEL}\` label instead.`,
        '',
        'Contract files changed:',
        ...result.contractFiles.map((f) => `  ${f}`),
      ].join('\n');
  }
}

function main() {
  const fs = require('node:fs');
  const path = process.argv[2];
  if (!path) {
    console.error('usage: PR_LABELS=<json array> node changelog-check.cjs <changed-files.txt>');
    process.exit(2);
  }
  const result = checkChangelog({
    files: parseFiles(fs.readFileSync(path, 'utf8')),
    labels: parseLabels(process.env.PR_LABELS),
  });
  const text = message(result);
  if (result.ok) {
    console.log(text);
    return;
  }
  // The annotation shows on the pull request's checks tab; the full text
  // (with the file list) is in the job log.
  console.log(`::error title=CHANGELOG.md not updated::${text.split('\n')[0]} Add an entry under "## Unreleased", or ask a maintainer for the ${SKIP_LABEL} label.`);
  console.log(text);
  process.exit(1);
}

module.exports = { checkChangelog, parseFiles, parseLabels, message, SKIP_LABEL };

if (require.main === module) main();
