// Re-sign remote staging locally: private keys never leave the user's keychain.
const { execFileSync } = require("node:child_process");
const { existsSync, realpathSync } = require("node:fs");
const path = require("node:path");
const {
  signingOptions,
  assertStableRequirement,
} = require("../desktop/packaging/signing.cjs");

function run(args) {
  return execFileSync("/usr/bin/codesign", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}
function requirement(app) {
  // codesign sends the requirement and executable diagnostics to separate streams.
  const { spawnSync } = require("node:child_process");
  const result = spawnSync("/usr/bin/codesign", ["-d", "-r-", app], {
    encoding: "utf8",
  });
  if (result.status !== 0) throw new Error(result.stderr);
  const value = `${result.stdout}\n${result.stderr}`.match(
    /(?:# )?designated => (.+)/,
  )?.[1];
  if (!value) throw new Error("Missing designated requirement");
  return value;
}

async function main() {
  const args = process.argv.slice(2);
  const migration = args.includes("--migrate-identity");
  const positional = args.filter((arg) => arg !== "--migrate-identity");
  if (
    process.platform !== "darwin" ||
    positional.length !== 1 ||
    !positional[0].endsWith(".app")
  ) {
    throw new Error(
      "Usage: node scripts/sign-local-macos.cjs <staging.app> [--migrate-identity]",
    );
  }
  const app = realpathSync(positional[0]);
  const installed = "/Applications/momo.app";
  if (app === installed || app.startsWith("/Applications/"))
    throw new Error("Sign a staging copy, never the installed app");
  const options = signingOptions(process.env.CSC_NAME);
  if (options.identity === "-")
    throw new Error("Ad-hoc candidates cannot be installed");
  const { sign } = await import("@electron/osx-sign");
  // The OS-facing helper also keeps its identity across rebuilds.
  const helper = path.join(
    app,
    "Contents/Resources/helpers/momo-native-control",
  );
  if (existsSync(helper))
    run([
      "--force",
      "--sign",
      options.identity,
      "--timestamp=none",
      "--identifier",
      "dev.momo.native-control",
      helper,
    ]);
  await sign({ app, ...options });
  run(["--verify", "--deep", "--strict", app]);
  const next = requirement(app);
  assertStableRequirement(next);
  let inheritedIdentity = false;
  if (existsSync(installed)) {
    const previous = requirement(installed);
    try {
      assertStableRequirement(previous);
      run(["--verify", "--strict", "-R", `=${previous}`, app]);
      inheritedIdentity = true;
    } catch (error) {
      if (!migration)
        throw new Error(
          "Installed identity differs. Stop: explicit --migrate-identity and a one-time OS reauthorization are required",
          { cause: error },
        );
    }
  }
  console.log(
    JSON.stringify(
      {
        app,
        requirement: next,
        inheritedIdentity,
        permissionInheritance: "Requires real macOS permission verification",
      },
      null,
      2,
    ),
  );
}
main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
