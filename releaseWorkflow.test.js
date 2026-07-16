import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('Release workflow 应在构建前执行测试并使用 npm ci', () => {
  const source = readFileSync(new URL('./.github/workflows/release.yml', import.meta.url), 'utf8');

  assert.match(source, /run:\s*npm ci/);
  assert.match(source, /name:\s*Run frontend tests/);
  assert.match(source, /run:\s*node --test/);
  assert.match(source, /name:\s*Build frontend assets for Rust tests/);
  assert.match(source, /run:\s*npm run build/);
  assert.match(source, /name:\s*Run Rust tests/);
  assert.match(source, /run:\s*cargo test --manifest-path src-tauri\/Cargo\.toml/);

  const frontendIndex = source.indexOf('name: Run frontend tests');
  const frontendBuildIndex = source.indexOf('name: Build frontend assets for Rust tests');
  const rustIndex = source.indexOf('name: Run Rust tests');
  const buildIndex = source.indexOf('name: Build application');

  assert.notEqual(frontendIndex, -1);
  assert.notEqual(frontendBuildIndex, -1);
  assert.notEqual(rustIndex, -1);
  assert.notEqual(buildIndex, -1);
  assert.ok(frontendIndex < buildIndex, '前端测试必须先于构建执行');
  assert.ok(frontendBuildIndex < rustIndex, 'Rust 测试前必须先生成 frontendDist');
  assert.ok(rustIndex < buildIndex, 'Rust 测试必须先于构建执行');
});

test('Release workflow 应构建并上传 Linux RPM 产物', () => {
  const source = readFileSync(new URL('./.github/workflows/release.yml', import.meta.url), 'utf8');

  assert.match(source, /args:\s*"--target x86_64-unknown-linux-gnu --bundles deb,rpm,appimage"[\s\S]*target:\s*x86_64-unknown-linux-gnu/);
  assert.match(source, /args:\s*"--target aarch64-unknown-linux-gnu --bundles deb"[\s\S]*target:\s*aarch64-unknown-linux-gnu/);
  assert.match(source, /sudo apt-get install -y[\s\S]*\brpm\b/);
  assert.match(source, /-name "\*\.rpm"/);
  assert.match(source, /release\/bundle\/rpm\/\*\.rpm/);
  assert.match(source, /require_file "\*\/release\/bundle\/rpm\/\*\.rpm" "Linux x64 RPM"/);
  assert.match(source, /target\/\*\*\/release\/bundle\/rpm\/\*\.rpm/);
});

test('Release workflow 应产出 Work Journal 的双架构 macOS 与 Windows 安装和便携包', () => {
  const source = readFileSync(new URL('./.github/workflows/release.yml', import.meta.url), 'utf8');

  assert.match(source, /target:\s*aarch64-apple-darwin/);
  assert.match(source, /target:\s*x86_64-apple-darwin/);
  assert.match(source, /target:\s*x86_64-pc-windows-msvc/);
  assert.match(source, /release\/bundle\/nsis\/\*\.exe/);
  assert.match(source, /-name "Work_Journal\.exe"/);
  assert.match(source, /Work_Journal_portable_x64\.zip/);
  assert.match(source, /tar -tf "\$PORTABLE_ZIP"/);
  assert.match(source, /for required_entry in Work_Journal\.exe PORTABLE_README\.txt/);
  assert.match(source, /UNEXPECTED_ENTRIES=/);
  assert.match(source, /target\/\*\*\/Work_Journal_portable_\*\.zip/);
  assert.match(source, /Applications\/Work Journal\.app/);
  assert.doesNotMatch(source, /Work_Review\.exe/);
  assert.doesNotMatch(source, /Work_Review_portable/);
});

test('Release workflow 应支持不发布 Release 的内部无签名构建', () => {
  const source = readFileSync(new URL('./.github/workflows/release.yml', import.meta.url), 'utf8');

  assert.match(source, /workflow_dispatch:/);
  assert.match(source, /github\.event_name.*workflow_dispatch/);
  assert.match(source, /--config src-tauri\/tauri\.local\.conf\.json/);
  assert.match(source, /if: github\.event_name == 'push' && startsWith\(github\.ref, 'refs\/tags\/v'\)/);
  assert.match(source, /require_file "\*\/Work_Journal_portable_x64\.zip" "Windows 便携版"/);
});
