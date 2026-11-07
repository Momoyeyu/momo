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

module.exports = { LOCAL_IDENTITY, signingOptions, assertStableRequirement };
