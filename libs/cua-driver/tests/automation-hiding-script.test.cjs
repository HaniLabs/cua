// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Cua AI, Inc.

// Independent controlled probe of the fixed script and its documented scope.
// This reconstructs the Rust wrapper; real browser integration is a separate test.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const sourcePath = path.join(__dirname, '../rust/crates/cua-driver-core/src/browser/stealth.rs');
const rust = fs.readFileSync(sourcePath, 'utf8');
const fixed = rust.match(/const HIDE_AUTOMATION_SCRIPT: &str = r#"([\s\S]*?)"#;/);
assert.ok(fixed, 'fixed source exists');
assert.match(rust, /format!\("if \(self === self\.top && location\.origin === \{scope\}\)/, 'guard builder has the intended scope');
const origin = 'https://fixture.test';
const script = `if (self === self.top && location.origin === ${JSON.stringify(origin)}) {\n${fixed[1]}\n}`;
function fixture(actualOrigin, isTop) {
  function Navigator() {}
  Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => true, configurable: true });
  function HTMLCanvasElement() {}
  HTMLCanvasElement.prototype.getContext = () => null;
  const navigator = new Navigator();
  navigator.plugins = [];
  const intervals = new Map();
  const timeouts = [];
  const ctx = {
    Navigator, navigator, HTMLCanvasElement, location: { origin: actualOrigin },
    Notification: { permission: 'default' },
    setInterval(fn, ms) { intervals.set(1, {fn, ms}); return 1; },
    clearInterval(id) { intervals.delete(id); },
    setTimeout(fn, ms) { timeouts.push({fn, ms}); return 2; },
  };
  ctx.self = ctx;
  ctx.top = isTop ? ctx : {};
  ctx.window = ctx;
  vm.runInNewContext(script, ctx, { timeout: 1000 });
  return { ctx, intervals, timeouts };
}
for (const [label, value, top] of [
  ['different origin', 'https://other.test', true],
  ['opaque origin', 'null', true],
  ['subframe', origin, false],
]) {
  test(`does not execute for ${label}`, () => {
    const {ctx, intervals, timeouts} = fixture(value, top);
    assert.equal(ctx.navigator.webdriver, true);
    assert.equal(ctx.chrome, undefined);
    assert.equal(intervals.size, 0);
    assert.equal(timeouts.length, 0);
  });
}
test('approved top-level script stops its periodic update after 15 seconds', () => {
  const {ctx, intervals, timeouts} = fixture(origin, true);
  assert.equal(ctx.navigator.webdriver, false);
  assert.ok(ctx.chrome);
  assert.equal(intervals.get(1).ms, 30);
  assert.equal(timeouts.length, 1);
  assert.equal(timeouts[0].ms, 15000);
  Object.defineProperty(ctx.Navigator.prototype, 'webdriver', { get: () => true, configurable: true });
  intervals.get(1).fn();
  assert.equal(ctx.navigator.webdriver, false);
  timeouts[0].fn();
  assert.equal(intervals.size, 0);
});
