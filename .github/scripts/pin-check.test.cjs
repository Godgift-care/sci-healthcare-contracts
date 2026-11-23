'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// A full commit SHA is exactly 40 lowercase hex characters.
const SHA40 = /^[0-9a-f]{40}$/;

test('all workflow actions are pinned to full commit SHAs', () => {
  const workflowDir = path.join(__dirname, '..', 'workflows');
  const files = fs.readdirSync(workflowDir).filter(f => f.endsWith('.yml'));

  const unpinned = [];
  for (const file of files) {
    const content = fs.readFileSync(path.join(workflowDir, file), 'utf8');
    const lines = content.split('\n');
    for (let i = 0; i < lines.length; i++) {
      // Match "uses: owner/repo@ref" (with optional leading whitespace).
      const m = lines[i].match(/^\s+uses:\s+([^\s#]+@([^\s#]+))/);
      if (!m) continue;
      const ref = m[2];
      if (!SHA40.test(ref)) {
        unpinned.push(`${file}:${i + 1}: ${m[1]}`);
      }
    }
  }

  assert.deepEqual(
    unpinned,
    [],
    `Unpinned actions found — pin each to a full 40-character commit SHA:\n${unpinned.join('\n')}`,
  );
});
