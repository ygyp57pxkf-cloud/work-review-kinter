import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('work journal commands should be exposed through Tauri invoke handler', () => {
  const commandsMod = readFileSync('src-tauri/src/commands/mod.rs', 'utf8');
  const main = readFileSync('src-tauri/src/main.rs', 'utf8');

  assert.match(commandsMod, /mod work_journal;/);
  assert.match(commandsMod, /pub use work_journal::\*;/);
  assert.match(main, /commands::get_work_journal_project_rules/);
  assert.match(main, /commands::save_work_journal_project_rules/);
  assert.match(main, /commands::match_work_journal_project/);
  assert.match(main, /commands::get_work_journal_day/);
  assert.match(main, /commands::preview_work_journal_obsidian_export/);
});
