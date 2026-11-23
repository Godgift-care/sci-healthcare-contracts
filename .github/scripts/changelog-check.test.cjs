const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const { checkChangelog, parseFiles, parseLabels, message, SKIP_LABEL } = require('./changelog-check.cjs');

test('docs-only pull requests are not affected', () => {
  const r = checkChangelog({ files: ['docs/guides/funders.md', 'README.md'], labels: [] });
  assert.deepEqual(r, { ok: true, reason: 'no-contract-change', contractFiles: [] });
});

test('CI, scripts and examples changes are not affected', () => {
  const files = ['.github/workflows/ci.yml', 'scripts/deploy.sh', 'examples/lifecycle.ts', 'Cargo.lock'];
  assert.equal(checkChangelog({ files, labels: [] }).ok, true);
});

test('an empty pull request passes', () => {
  assert.equal(checkChangelog({ files: [], labels: [] }).reason, 'no-contract-change');
});

test('a contract change with a changelog entry passes', () => {
  const r = checkChangelog({ files: ['contracts/voucher/src/lib.rs', 'CHANGELOG.md'], labels: [] });
  assert.equal(r.ok, true);
  assert.equal(r.reason, 'changelog-updated');
  assert.deepEqual(r.contractFiles, ['contracts/voucher/src/lib.rs']);
});

test('a contract change without a changelog entry fails and lists the files', () => {
  const files = ['contracts/registry/src/lib.rs', 'contracts/registry/test_snapshots/x.json', 'docs/x.md'];
  const r = checkChangelog({ files, labels: ['area: docs'] });
  assert.equal(r.ok, false);
  assert.equal(r.reason, 'changelog-missing');
  assert.deepEqual(r.contractFiles, ['contracts/registry/src/lib.rs', 'contracts/registry/test_snapshots/x.json']);
  const text = message(r);
  assert.match(text, /contracts\/registry\/src\/lib\.rs/);
  assert.match(text, /skip-changelog/);
});

test('the skip-changelog label lets a contract change through', () => {
  const r = checkChangelog({ files: ['contracts/common/src/lib.rs'], labels: ['contract: common', SKIP_LABEL] });
  assert.deepEqual(r, { ok: true, reason: 'skipped-by-label', contractFiles: ['contracts/common/src/lib.rs'] });
});

test('only the exact label name skips the check', () => {
  const r = checkChangelog({ files: ['contracts/common/src/lib.rs'], labels: ['skip changelog', 'Skip-Changelog'] });
  assert.equal(r.ok, false);
});

test('a changelog somewhere else does not count', () => {
  const r = checkChangelog({ files: ['contracts/voucher/src/lib.rs', 'docs/CHANGELOG.md'], labels: [] });
  assert.equal(r.ok, false);
});

test('paths that merely start with "contracts" are not contract changes', () => {
  const r = checkChangelog({ files: ['contracts.md', 'docs/contracts/notes.md'], labels: [] });
  assert.equal(r.reason, 'no-contract-change');
});

test('parseFiles ignores blank lines and whitespace', () => {
  assert.deepEqual(parseFiles('a\n\n  contracts/x.rs  \n'), ['a', 'contracts/x.rs']);
});

test('parseLabels reads a JSON array and treats empty input as no labels', () => {
  assert.deepEqual(parseLabels('["a","skip-changelog"]'), ['a', 'skip-changelog']);
  assert.deepEqual(parseLabels(''), []);
  assert.deepEqual(parseLabels(undefined), []);
  assert.deepEqual(parseLabels('[]'), []);
});

test('parseLabels rejects anything but an array', () => {
  assert.throws(() => parseLabels('{"name":"x"}'), /JSON array/);
  assert.throws(() => parseLabels('not json'));
});

function run(files, labels) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'changelog-check-'));
  const list = path.join(dir, 'files.txt');
  fs.writeFileSync(list, files.join('\n'));
  try {
    const out = execFileSync(process.execPath, [path.join(__dirname, 'changelog-check.cjs'), list], {
      env: { ...process.env, PR_LABELS: JSON.stringify(labels) },
      encoding: 'utf8',
    });
    return { code: 0, out };
  } catch (err) {
    return { code: err.status, out: err.stdout };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('the CLI exits 0 when the check passes', () => {
  const r = run(['docs/a.md'], []);
  assert.equal(r.code, 0);
  assert.match(r.out, /no changelog entry needed/);
});

test('the CLI exits 1 with an error annotation when the entry is missing', () => {
  const r = run(['contracts/voucher/src/lib.rs'], []);
  assert.equal(r.code, 1);
  assert.match(r.out, /^::error title=CHANGELOG\.md not updated::/m);
  assert.match(r.out, /contracts\/voucher\/src\/lib\.rs/);
});

test('the CLI exits 0 when skipped by label', () => {
  const r = run(['contracts/voucher/src/lib.rs'], [SKIP_LABEL]);
  assert.equal(r.code, 0);
  assert.match(r.out, /skipped by the `skip-changelog` label/);
});
