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
