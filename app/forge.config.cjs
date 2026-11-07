const path = require("node:path");
const { validatedSigningOptions } = require("./desktop/packaging/signing.cjs");
const {
  normalizeResourceLinks,
  resourceNames,
} = require("./desktop/packaging/resources.cjs");
const { FusesPlugin } = require("@electron-forge/plugin-fuses");
const { FuseV1Options, FuseVersion } = require("@electron/fuses");

module.exports = {
  packagerConfig: {
    name: "momo",
    executableName: "momo",
    appBundleId: "dev.momo.electron",
    appCategoryType: "public.app-category.games",
    asar: true,
    // CSC_NAME=- is only for remote staging; sign locally before installation.
    osxSign: validatedSigningOptions(process.env.CSC_NAME),
    icon: path.join(__dirname, "desktop/assets/icon"),
    extraResource: resourceNames.map((name) =>
      path.join(__dirname, "resources", name),
    ),
    afterCopyExtraResources: [
      async ({ buildPath, platform }) =>
        normalizeResourceLinks(
          path.join(__dirname, "resources"),
          path.join(
            buildPath,
            platform === "darwin" ? "momo.app/Contents/Resources" : "resources",
          ),
        ),
    ],
    extendInfo: {
      NSMicrophoneUsageDescription:
        "momo uses your microphone when you start voice input.",
      LSMinimumSystemVersion: "14.0",
    },
    ignore: (file) =>
      file !== "" &&
      !/^\/(package\.json|migration($|\/web-source\.json$)|\.desktop($|\/(main|preload)(\/|$)|\/contracts\.js$)|frontend($|\/dist(\/|$)))/.test(
        file,
      ),
  },
  makers: [
    { name: "@electron-forge/maker-zip", platforms: ["darwin"] },
    {
      name: "@electron-forge/maker-dmg",
      config: { format: "ULFO" },
      platforms: ["darwin"],
    },
  ],
  plugins: [
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};
