import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('app shell should expose Journal as a first-class route and sidebar item', () => {
  const appSource = readFileSync('src/App.svelte', 'utf8');
  const sidebarSource = readFileSync('src/lib/components/Sidebar.svelte', 'utf8');

  assert.match(appSource, /'\/journal':\s*wrap\(\{ asyncComponent: \(\) => import\('\.\/routes\/journal\/Journal\.svelte'\) \}\)/);
  assert.match(sidebarSource, /path:\s*'\/journal'/);
  assert.match(sidebarSource, /labelKey:\s*'sidebar\.nav\.journal'/);
});

test('Journal page should load daily sessions and preview Obsidian export without writing files', () => {
  const source = readFileSync('src/routes/journal/Journal.svelte', 'utf8');

  assert.match(source, /invoke\('get_work_journal_day'/);
  assert.match(source, /invoke\('preview_work_journal_obsidian_export'/);
  assert.doesNotMatch(source, /invoke\('export_work_journal_obsidian'/);
  assert.match(source, /journal-export-preview/);
});
