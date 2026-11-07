// A stable certificate is required for local upgrades. Ad-hoc is staging only.
const LOCAL_IDENTITY = "momo Local Development";

function signingOptions(identity = LOCAL_IDENTITY) {
  if (!identity.trim()) throw new Error("A signing identity is required");
  return {
    identity,
    identityValidation: identity !== "-",
    gatekeeperAssess: false,
    preEmbedProvisioningProfile: false,
    preAutoEntitlements: false,
    ignore: (file) => file.includes("/Contents/Resources/"),
    optionsForFile: () => ({ hardenedRuntime: false, timestamp: "none" }),
  };
}

function assertStableRequirement(requirement) {
  if (
    !requirement.includes('identifier "dev.momo.electron"') ||
    !/\b(anchor|certificate)\b/.test(requirement) ||
    /\bcdhash\b/.test(requirement)
  ) {
    throw new Error(
      "Refusing an update without a certificate-bound momo identity",
    );
  }
}

function resolveCodeSigningIdentity(listing, requested) {
  const found = new Set();
  for (const line of listing.split("\n")) {
    const match = line.match(/\)\s+([A-Fa-f0-9]{40})\s+"([^"]+)"/);
    if (
      match &&
      (match[2] === requested ||
        match[1].toLowerCase() === requested.toLowerCase())
    )
      found.add(match[1].toUpperCase());
  }
  if (found.size !== 1)
    throw new Error("Expected exactly one valid code-signing identity");
  return [...found][0];
}

function validatedSigningOptions(identity = LOCAL_IDENTITY) {
  const options = signingOptions(identity);
  if (identity === "-" || process.platform !== "darwin") return options;
  const { execFileSync } = require("node:child_process");
  const listing = execFileSync(
    "/usr/bin/security",
    ["find-identity", "-v", "-p", "codesigning"],
    { encoding: "utf8" },
  );
  // osx-sign's generic-policy lookup rejects a code-signing-only trust policy.
  // Validate under the correct OS policy here, then supply its exact fingerprint.
  return {
    ...options,
    identity: resolveCodeSigningIdentity(listing, identity),
    identityValidation: false,
  };
}

module.exports = {
  LOCAL_IDENTITY,
  signingOptions,
  validatedSigningOptions,
  resolveCodeSigningIdentity,
  assertStableRequirement,
};
