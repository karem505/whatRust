// Run: node --test scripts/install.test.mjs
// Exercise the real installer without network access or writes to /Applications.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const installer = readFileSync(new URL('../install.sh', import.meta.url), 'utf8');
const stubs = `
mktemp() { /usr/bin/mktemp -d "$TMPDIR/install.XXXXXX"; }
uname() {
  case "$1" in -s) echo Darwin ;; -m) echo arm64 ;; esac
}
curl() {
  case "$*" in
    *progress-bar*)
      while [ "$1" != -o ]; do shift; done
      : > "$2"
      ;;
    *) echo '{"browser_download_url":"https://example.invalid/whatRust_aarch64.dmg"}' ;;
  esac
}
hdiutil() {
  operation=$1; shift
  case "$operation" in
    attach)
      [ "$SCENARIO" != attach-failure ] || return 1
      mount_path="$TEST_ROOT/Volumes/whatRust 1"
      while [ "$#" -gt 0 ]; do
        if [ "$1" = -mountpoint ]; then mount_path=$2; shift; fi
        shift
      done
      mkdir -p "$mount_path"
      printf '%s' "$mount_path" > "$TEST_ROOT/mounted"
      if [ "$SCENARIO" != missing-bundle ]; then
        mkdir -p "$mount_path/whatRust.app"
        : > "$mount_path/whatRust.app/release-marker"
      fi
      printf '/dev/disk99s1 Apple_HFS %s\\n' "$mount_path"
      ;;
    detach)
      [ -f "$TEST_ROOT/mounted" ] || return 1
      [ "$1" = "$(cat "$TEST_ROOT/mounted")" ] || return 1
      /bin/rm "$TEST_ROOT/mounted"
      ;;
    *) return 1 ;;
  esac
}
cp() {
  [ "$SCENARIO" != copy-failure ] || return 1
  [ "$1" = -R ] && [ -f "$2/release-marker" ] && [ "$3" = /Applications/ ] || return 1
  : > "$TEST_ROOT/installed"
}
rm() {
  if [ "$1" = -rf ] && [ "$2" = /Applications/whatRust.app ]; then return 0; fi
  case "$2" in "$TEST_ROOT"/*) /bin/rm "$@" ;; *) return 1 ;; esac
}
xattr() { :; }
`;

for (const scenario of ['success', 'missing-bundle', 'copy-failure', 'attach-failure']) {
  test(`macOS installer: ${scenario}, with spaces in temporary paths`, () => {
    const root = mkdtempSync(join(tmpdir(), 'whatrust installer test '));
    const temporary = join(root, 'temporary files');
    mkdirSync(temporary);
    try {
      const result = spawnSync('/bin/sh', [], {
        input: stubs + '\n' + installer,
        env: { ...process.env, TEST_ROOT: root, TMPDIR: temporary, SCENARIO: scenario },
        encoding: 'utf8',
        timeout: 10000,
      });
      assert.ifError(result.error);
      const output = result.stdout + result.stderr;
      if (scenario === 'success') {
        assert.equal(result.status, 0, output);
        assert.ok(existsSync(join(root, 'installed')), output);
      } else {
        assert.notEqual(result.status, 0, output);
        assert.ok(!existsSync(join(root, 'installed')), output);
      }
      assert.ok(!existsSync(join(root, 'mounted')), `image left mounted: ${output}`);
      assert.deepEqual(readdirSync(temporary), [], `temporary files left behind: ${output}`);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
