import { test } from "node:test";
import assert from "node:assert/strict";
const {
  signingOptions,
  assertStableRequirement,
} = require("../../desktop/packaging/signing.cjs");

test("local builds require a validated stable identity; ad-hoc staging is explicit", () => {
  assert.equal(signingOptions().identity, "momo Local Development");
  assert.equal(signingOptions().identityValidation, true);
  assert.equal(signingOptions("-").identityValidation, false);
  assert.throws(() => signingOptions(""));
});
test("installation rejects ad-hoc and identifier-only identity", () => {
  assert.throws(() => assertStableRequirement('cdhash H"1234"'));
  assert.throws(() =>
    assertStableRequirement('identifier "dev.momo.electron"'),
  );
  assert.throws(() =>
    assertStableRequirement('identifier "other.app" and anchor H"1234"'),
  );
  assert.doesNotThrow(() =>
    assertStableRequirement(
      'identifier "dev.momo.electron" and anchor H"1234"',
    ),
  );
});
