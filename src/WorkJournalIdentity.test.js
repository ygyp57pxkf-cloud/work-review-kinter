import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('fork identity should be Work Journal and use a separate bundle identifier', async () => {
  const packageJson = JSON.parse(
    await readFile(new URL('../package.json', import.meta.url), 'utf8')
  );
  const tauriConfig = JSON.parse(
    await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')
  );

  assert.equal(packageJson.name, 'work-journal');
  assert.equal(tauriConfig.productName, 'Work Journal');
  assert.equal(tauriConfig.mainBinaryName, 'Work_Journal');
  assert.equal(tauriConfig.identifier, 'com.kinter.workjournal');
  assert.notEqual(tauriConfig.identifier, 'com.workreview.app');

  const updaterEndpoints = tauriConfig.plugins.updater.endpoints.join('\n');
  assert.match(updaterEndpoints, /ygyp57pxkf-cloud\/work-review-kinter/);
  assert.doesNotMatch(updaterEndpoints, /wm94i\/Work-Review/);
});

test('macOS permission copy should name the forked app', async () => {
  const infoPlist = await readFile(
    new URL('../src-tauri/Info.plist', import.meta.url),
    'utf8'
  );

  assert.match(infoPlist, /Work Journal 需要屏幕录制权限/);
  assert.match(infoPlist, /Work Journal 需要自动化权限/);
  assert.doesNotMatch(infoPlist, /Work Review 需要屏幕录制权限/);
});

test('primary visible brand should use Work Journal', async () => {
  const sidebar = await readFile(
    new URL('../src/lib/components/Sidebar.svelte', import.meta.url),
    'utf8'
  );
  const about = await readFile(
    new URL('../src/routes/about/About.svelte', import.meta.url),
    'utf8'
  );

  assert.match(sidebar, />Work Journal</);
  assert.match(about, />Work Journal</);
});
