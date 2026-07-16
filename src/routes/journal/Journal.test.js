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
  assert.match(source, /preview\.confirmation_token/);
  assert.match(source, /preview\.content_hash/);
  assert.match(source, /journal-export-preview/);
});

test('Journal page should review, privatize, and exclude sessions through a persisted command', () => {
  const source = readFileSync('src/routes/journal/Journal.svelte', 'utf8');

  assert.match(source, /invoke\('review_work_journal_session'/);
  assert.match(source, /review_state:\s*reviewState/);
  assert.match(source, /journal\.confirmSession/);
  assert.match(source, /journal\.markPrivate/);
  assert.match(source, /journal\.excludeSession/);
});

test('Journal page should only export after an explicit confirmation action', () => {
  const source = readFileSync('src/routes/journal/Journal.svelte', 'utf8');

  assert.match(source, /async function exportToObsidian/);
  assert.match(source, /await ask\(/);
  assert.match(source, /invoke\('export_work_journal_obsidian'/);
  assert.match(source, /\{ input: previewConfirmation \}/);
  assert.match(source, /journal\.confirmExport/);
});

test('Journal page should replace automatic export with copy in manual mode', () => {
  const source = readFileSync('src/routes/journal/Journal.svelte', 'utf8');

  assert.match(source, /conflict_behavior === 'manual_copy'/);
  assert.match(source, /navigator\.clipboard\.writeText\(previewMarkdown\)/);
  assert.match(source, /\{#if obsidianManualCopyEnabled\}/);
  assert.match(source, /\{:else if obsidianWriteEnabled\}/);
});

test('Journal page should only run AI attribution from an explicit action', () => {
  const source = readFileSync('src/routes/journal/Journal.svelte', 'utf8');

  assert.match(source, /async function analyzeWithAi/);
  assert.match(source, /invoke\('analyze_work_journal_with_ai'/);
  assert.match(source, /journal\.analyzeWithAi/);
  assert.match(source, /work_journal_ai\?\.enabled/);
});
