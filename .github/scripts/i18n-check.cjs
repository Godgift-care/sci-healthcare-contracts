'use strict';

// Checks that every translated documentation page declares its counterparts
// in front matter, that each declares the others back, and that the links
// point at pages that exist. Run by the `bot` job in ci.yml, and by
// `.github/workflows/docs-site.yml` through the include it validates.
//
// Front matter a translated page carries:
//
//   language: fr
//   translations:
//     en: /guides/funders.html
//     sw: /sw/funders.html
//
// The URL is the page's published path. Docs pages build to <name>.html;
// scripts/build-docs-site.sh passes the base path to jekyll and the include
// runs the value through `relative_url`.
//
// Language roots: English pages anywhere under docs/ except the translation
// directories; French under docs/fr/, Swahili under docs/sw/, Portuguese under
// docs/pt/, Yoruba under docs/yo/ and Igbo under docs/ig/.

const fs = require('node:fs');
const path = require('node:path');

const LANGUAGES = ['en', 'fr', 'sw', 'pt', 'yo', 'ig'];
const LANGUAGE_DIRS = { en: '', fr: 'fr', sw: 'sw', pt: 'pt', yo: 'yo', ig: 'ig' };
const SKIP_DIRS = new Set(['_includes', '_data', '_layouts', '_site', 'node_modules']);

function unquote(value) {
  const trimmed = value.trim();
  if (
    trimmed.length >= 2 &&
    (trimmed[0] === '"' || trimmed[0] === "'") &&
    trimmed.at(-1) === trimmed[0]
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

/**
 * Reads the front matter values this check needs. Deliberately a small parser
 * for the shape above rather than a YAML dependency.
 *
 * @param {string} source the Markdown file
 * @returns {{language: string|null, permalink: string|null, translations: Object<string,string>}|null}
 */
function parseFrontMatter(source) {
  const lines = source.split(/\r?\n/);
  if (lines[0].trim() !== '---') return null;
  const end = lines.indexOf('---', 1);
  if (end === -1) return null;

  const matter = { language: null, permalink: null, translations: {} };
  let inTranslations = false;

  for (const line of lines.slice(1, end)) {
    if (!line.trim() || line.trimStart().startsWith('#')) continue;

    if (!/^\s/.test(line)) {
      inTranslations = false;
      const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
      if (!match) continue;
      const [, key, raw] = match;
      if (key === 'language') matter.language = unquote(raw);
      else if (key === 'permalink') matter.permalink = unquote(raw);
      else if (key === 'translations' && !raw.trim()) inTranslations = true;
      continue;
    }

    if (!inTranslations) continue;
    const match = line.match(/^\s+([A-Za-z-]+):\s*(\S+)\s*$/);
    if (match) matter.translations[match[1]] = unquote(match[2]);
  }

  return matter;
}

function walk(dir, files = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.') || SKIP_DIRS.has(entry.name)) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, files);
    else if (entry.name.endsWith('.md')) files.push(full);
  }
  return files.sort();
}

function languageOf(relativePath) {
  const root = relativePath.split('/')[0];
  return LANGUAGES.includes(root) && root !== 'en' ? root : 'en';
}

function pageUrl(relativePath, matter) {
  if (matter && matter.permalink) return matter.permalink;
  return '/' + relativePath.replace(/\.md$/, '.html');
}

/** The Markdown file a published URL is built from. */
function fileForUrl(docsRoot, url) {
  const withoutHtml = url.replace(/\.html$/, '');
  const parts = withoutHtml.split('/').filter(Boolean);
  return path.join(docsRoot, ...parts) + '.md';
}

function loadPages(docsRoot) {
  const pages = new Map();
  for (const file of walk(docsRoot)) {
    const relative = path.relative(docsRoot, file).split(path.sep).join('/');
    const matter = parseFrontMatter(fs.readFileSync(file, 'utf8'));
    pages.set(relative, {
      relative,
      language: languageOf(relative),
      matter,
      url: pageUrl(relative, matter),
    });
  }
  return pages;
}

/**
 * @param {string} docsRoot the docs/ directory
 * @returns {{ok: boolean, errors: string[]}}
 */
function checkDocs(docsRoot) {
  const pages = loadPages(docsRoot);
  const errors = [];
  const relativeOf = (file) => path.relative(docsRoot, file).split(path.sep).join('/');

  for (const page of pages.values()) {
    const translated = page.language !== 'en';

    if (!page.matter) {
      if (translated) {
        errors.push(
          `${page.relative}: translated page has no front matter; add language: ${page.language} and translations:`,
        );
      }
      continue;
    }

    if (!page.matter.language) {
      if (translated) errors.push(`${page.relative}: missing language: ${page.language} in front matter`);
    } else if (page.matter.language !== page.language) {
      const where = page.language === 'en' ? 'the English tree' : `docs/${page.language}/`;
      errors.push(`${page.relative}: language is "${page.matter.language}" but the file is under ${where}`);
    }

    const declared = Object.entries(page.matter.translations);
    if (translated && declared.length === 0) {
      errors.push(`${page.relative}: no translations; link every counterpart that exists`);
    }

    for (const [language, url] of declared) {
      if (!LANGUAGES.includes(language)) {
        errors.push(
          `${page.relative}: unknown translation language "${language}" (expected ${LANGUAGES.join(', ')})`,
        );
        continue;
      }
      if (language === page.language) {
        errors.push(`${page.relative}: translations.${language} links to its own language`);
        continue;
      }
      if (!url.startsWith('/')) {
        errors.push(
          `${page.relative}: translations.${language} must be a site path starting with "/" (got "${url}")`,
        );
        continue;
      }
      if (url.includes('..')) {
        errors.push(`${page.relative}: translations.${language} must not contain ".." (got "${url}")`);
        continue;
      }
      if (!url.endsWith('.html')) {
        errors.push(`${page.relative}: translations.${language} must end in .html (got "${url}")`);
        continue;
      }
      const targetFile = fileForUrl(docsRoot, url);
      if (!fs.existsSync(targetFile)) {
        errors.push(`${page.relative}: translations.${language} points at ${url}, but that page does not exist`);
        continue;
      }
      const targetRelative = relativeOf(targetFile);
      if (languageOf(targetRelative) !== language) {
        const dir = LANGUAGE_DIRS[language] ? `docs/${LANGUAGE_DIRS[language]}/` : 'docs/';
        errors.push(`${page.relative}: translations.${language} points at ${url}, which is not under ${dir}`);
        continue;
      }
      const target = pages.get(targetRelative);
      if (!target || !target.matter || !target.matter.language) {
        errors.push(`${targetRelative}: missing language: ${language} (linked from ${page.relative})`);
        continue;
      }
      if (target.matter.language !== language) {
        errors.push(
          `${targetRelative}: language "${target.matter.language}" does not match translations.${language} in ${page.relative}`,
        );
        continue;
      }
      if (!target.matter.translations[page.language]) {
        errors.push(`${targetRelative}: missing translations.${page.language}; ${page.relative} links to it`);
        continue;
      }
      if (target.matter.translations[page.language] !== page.url) {
        errors.push(
          `${targetRelative}: translations.${page.language} points at ${target.matter.translations[page.language]}, not ${page.url}`,
        );
      }
    }
  }

  // Pages connected by links form one translation group, and every member
  // must link every other member. The per-link checks above catch a one-way
  // link; this catches a missing arm of a three-language group: English
  // links French, French links Swahili, but English never links Swahili.
  const parent = new Map([...pages.keys()].map((key) => [key, key]));
  const find = (key) => {
    let root = key;
    while (parent.get(root) !== root) root = parent.get(root);
    return root;
  };
  const union = (a, b) => {
    const rootA = find(a);
    const rootB = find(b);
    if (rootA !== rootB) parent.set(rootA, rootB);
  };

  for (const page of pages.values()) {
    if (!page.matter) continue;
    for (const [language, url] of Object.entries(page.matter.translations)) {
      if (!url.startsWith('/') || url.includes('..') || !url.endsWith('.html')) continue;
      const targetFile = fileForUrl(docsRoot, url);
      if (!fs.existsSync(targetFile)) continue;
      const target = pages.get(relativeOf(targetFile));
      if (!target || target.language !== language) continue;
      union(page.relative, target.relative);
    }
  }

  const groups = new Map();
  for (const key of pages.keys()) {
    const root = find(key);
    if (!groups.has(root)) groups.set(root, []);
    groups.get(root).push(pages.get(key));
  }

  for (const members of groups.values()) {
    if (members.length < 2) continue;

    const seen = new Map();
    for (const member of members) {
      if (seen.has(member.language)) {
        errors.push(
          `${member.relative}: links into the same translation group as ${seen.get(member.language)}, which is also ${member.language}`,
        );
      } else {
        seen.set(member.language, member.relative);
      }
    }

    for (const page of members) {
      for (const counterpart of members) {
        if (page === counterpart || page.language === counterpart.language) continue;
        const url = page.matter && page.matter.translations[counterpart.language];
        if (url !== counterpart.url) {
          errors.push(`${page.relative}: must link ${counterpart.language} counterpart ${counterpart.url}`);
        }
      }
    }
  }

  return { ok: errors.length === 0, errors };
}

function main() {
  const docsRoot = path.resolve(process.argv[2] || 'docs');
  const result = checkDocs(docsRoot);
  if (result.ok) {
    console.log(`Translation links are complete and symmetric in ${docsRoot}.`);
    return;
  }
  console.log(
    `::error title=Translation links incomplete::${result.errors.length} problem(s). Add the missing counterpart links in front matter.`,
  );
  for (const error of result.errors) console.log(`  ${error}`);
  process.exit(1);
}

module.exports = { checkDocs, parseFrontMatter, LANGUAGES };

if (require.main === module) main();
