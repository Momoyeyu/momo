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

const {
  resolveCodeSigningIdentity,
} = require("../../desktop/packaging/signing.cjs");
test("code-signing policy lookup rejects missing and ambiguous identities", () => {
  const a = "A".repeat(40),
    b = "B".repeat(40);
  const line = `1) ${a} "momo Local Development"`;
  assert.equal(resolveCodeSigningIdentity(line, "momo Local Development"), a);
  assert.equal(resolveCodeSigningIdentity(line, a.toLowerCase()), a);
  assert.throws(() =>
    resolveCodeSigningIdentity(
      "0 valid identities found",
      "momo Local Development",
    ),
  );
  assert.throws(() =>
    resolveCodeSigningIdentity(
      `${line}\n2) ${b} "momo Local Development"`,
      "momo Local Development",
    ),
  );
});
