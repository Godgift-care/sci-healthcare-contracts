const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const { checkDocs, parseFrontMatter } = require('./i18n-check.cjs');

function writeDocs(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'i18n-check-'));
  for (const [name, source] of Object.entries(files)) {
    const file = path.join(root, ...name.split('/'));
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, source);
  }
  return root;
}

function withDocs(files, fn) {
  const root = writeDocs(files);
  try {
    return fn(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function page(language, translations) {
  const lines = ['---', `language: ${language}`, 'translations:'];
  for (const [code, url] of Object.entries(translations)) lines.push(`  ${code}: ${url}`);
  lines.push('---', '', '# Page', '');
  return lines.join('\n');
}

function triangle() {
  return {
    'guides/funders.md': page('en', { fr: '/fr/funders.html', sw: '/sw/funders.html' }),
    'fr/funders.md': page('fr', { en: '/guides/funders.html', sw: '/sw/funders.html' }),
    'sw/funders.md': page('sw', { en: '/guides/funders.html', fr: '/fr/funders.html' }),
  };
}

test('parseFrontMatter reads language and translations, including CRLF', () => {
  const matter = parseFrontMatter(
    '---\r\ntitle: X\r\nlanguage: fr\r\ntranslations:\r\n  en: /guides/x.html\r\n---\r\n# X\r\n',
  );
  assert.equal(matter.language, 'fr');
  assert.deepEqual(matter.translations, { en: '/guides/x.html' });
});

test('parseFrontMatter returns null without front matter', () => {
  assert.equal(parseFrontMatter('# Just a page\n'), null);
  assert.equal(parseFrontMatter('---\ntitle: X\n'), null);
});

test('a complete three-language group passes', () => {
  const result = withDocs(triangle(), (root) => checkDocs(root));
  assert.deepEqual(result.errors, []);
  assert.equal(result.ok, true);
});

test('a two-language pair with no third translation passes', () => {
  const result = withDocs(
    {
      'protocol/economics.md': page('en', { sw: '/sw/protocol/economics.html' }),
      'sw/protocol/economics.md': page('sw', { en: '/protocol/economics.html' }),
    },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, true, result.errors.join('\n'));
});

test('a counterpart that does not link back fails', () => {
  const result = withDocs(
    {
      'guides/funders.md': page('en', { fr: '/fr/funders.html' }),
      'fr/funders.md': '---\nlanguage: fr\n---\n\n# Page\n',
    },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('fr/funders.md: missing translations.en')));
});

test('a missing arm of a three-language group fails', () => {
  const result = withDocs(
    {
      'guides/funders.md': page('en', { fr: '/fr/funders.html' }),
      'fr/funders.md': page('fr', { en: '/guides/funders.html', sw: '/sw/funders.html' }),
      'sw/funders.md': page('sw', { fr: '/fr/funders.html' }),
    },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('guides/funders.md: must link sw counterpart /sw/funders.html')));
  assert.ok(result.errors.some((e) => e.includes('sw/funders.md: must link en counterpart /guides/funders.html')));
});

test('a link to a page that does not exist fails', () => {
  const result = withDocs(
    {
      'guides/funders.md': page('en', { fr: '/fr/funders.html' }),
    },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('points at /fr/funders.html, but that page does not exist')));
});

test('a translated page without front matter fails', () => {
  const result = withDocs({ 'fr/funders.md': '# Financement\n' }, (root) => checkDocs(root));
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('fr/funders.md: translated page has no front matter')));
});

test('a translated page with no translations fails', () => {
  const result = withDocs({ 'fr/funders.md': '---\nlanguage: fr\n---\n' }, (root) => checkDocs(root));
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('fr/funders.md: no translations')));
});

test('a page linking its own language fails', () => {
  const result = withDocs(
    { 'guides/funders.md': page('en', { en: '/guides/funders.html' }) },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('translations.en links to its own language')));
});

test('a link outside the counterpart language root fails', () => {
  const result = withDocs(
    {
      'guides/funders.md': page('en', { fr: '/sw/funders.html' }),
      'sw/funders.md': page('sw', { en: '/guides/funders.html' }),
    },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('which is not under docs/fr/')));
});

test('two pages of the same language in one group fail', () => {
  const result = withDocs(
    {
      'guides/a.md': page('en', { fr: '/fr/a.html' }),
      'guides/b.md': page('en', { fr: '/fr/a.html' }),
      'fr/a.md': page('fr', { en: '/guides/a.html' }),
    },
    (root) => checkDocs(root),
  );
  assert.equal(result.ok, false);
  assert.ok(result.errors.some((e) => e.includes('which is also en')));
});

test('the repository docs pass', () => {
  const result = checkDocs(path.join(__dirname, '..', '..', 'docs'));
  assert.deepEqual(result.errors, []);
});

test('the CLI exits 0 on a complete set', () => {
  withDocs(triangle(), (root) => {
    const out = execFileSync(process.execPath, [path.join(__dirname, 'i18n-check.cjs'), root], {
      encoding: 'utf8',
    });
    assert.match(out, /Translation links are complete/);
  });
});

test('the CLI exits 1 with an annotation on a missing link', () => {
  withDocs(
    {
      'guides/funders.md': page('en', { fr: '/fr/funders.html' }),
      'fr/funders.md': page('fr', {}),
    },
    (root) => {
      assert.throws(
        () =>
          execFileSync(process.execPath, [path.join(__dirname, 'i18n-check.cjs'), root], {
            encoding: 'utf8',
          }),
        (err) => {
          assert.equal(err.status, 1);
          assert.match(err.stdout, /^::error title=Translation links incomplete::/m);
          assert.match(err.stdout, /fr\/funders\.md: no translations/);
          return true;
        },
      );
    },
  );
});
