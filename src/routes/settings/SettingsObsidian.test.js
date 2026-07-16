import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('settings should expose Work Journal project mapping tab', () => {
  const settingsSource = readFileSync('src/routes/settings/Settings.svelte', 'utf8');

  assert.match(settingsSource, /import SettingsObsidian from '\.\/components\/SettingsObsidian\.svelte';/);
  assert.match(settingsSource, /labelKey:\s*'settings\.tabs\.workJournal'/);
  assert.match(settingsSource, /<SettingsObsidian\s+bind:config/);
});

test('SettingsObsidian should use dedicated work journal project-rule commands', () => {
  const source = readFileSync('src/routes/settings/components/SettingsObsidian.svelte', 'utf8');

  assert.match(source, /invoke\('get_work_journal_project_rules'\)/);
  assert.match(source, /invoke\('save_work_journal_project_rules'/);
  assert.match(source, /work_journal_project_rules/);
});

test('SettingsObsidian should configure a local vault and explicit export policy', () => {
  const source = readFileSync('src/routes/settings/components/SettingsObsidian.svelte', 'utf8');

  assert.match(source, /invoke\('save_work_journal_obsidian_settings'/);
  assert.match(source, /work_journal_obsidian\.vault_path/);
  assert.match(source, /work_journal_obsidian\.daily_folder/);
  assert.match(source, /work_journal_obsidian\.export_mode/);
  assert.match(source, /work_journal_obsidian\.conflict_behavior/);
});

test('SettingsObsidian should keep text and vision attribution explicitly opt-in', () => {
  const source = readFileSync('src/routes/settings/components/SettingsObsidian.svelte', 'utf8');

  assert.match(source, /invoke\('save_work_journal_ai_settings'/);
  assert.match(source, /work_journal_ai\.enabled/);
  assert.match(source, /work_journal_ai\.vision_enabled/);
  assert.match(source, /work_journal_ai\.confidence_threshold/);
  assert.match(source, /work_journal_ai\.max_sessions_per_run/);
});

test('SettingsObsidian should preview and explicitly confirm legacy Work Review import', () => {
  const source = readFileSync('src/routes/settings/components/SettingsObsidian.svelte', 'utf8');

  assert.match(source, /invoke\('preview_work_review_import'/);
  assert.match(source, /invoke\('import_work_review_data'/);
  assert.match(source, /await ask\(/);
  assert.match(source, /confirmation_token: importPreview\.confirmation_token/);
  assert.match(source, /work-review-import-preview/);
});
