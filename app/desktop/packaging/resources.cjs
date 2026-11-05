const fs = require("node:fs/promises");
const path = require("node:path");

const resourceNames = [
  "backend",
  "models",
  "node",
  "plugin-host",
  "helpers",
  "qq",
];

async function normalizeResourceLinks(sourceRoot, destinationRoot) {
  sourceRoot = path.resolve(sourceRoot);
  destinationRoot = await fs.realpath(destinationRoot);
  const links = [];
  async function visit(directory) {
    for (const item of await fs.readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, item.name);
      if (item.isDirectory()) await visit(file);
      else if (item.isSymbolicLink()) {
        const link = await fs.readlink(file);
        let target = path.resolve(directory, link);
        if (path.isAbsolute(link) && link.startsWith(sourceRoot + path.sep))
          target = path.join(destinationRoot, path.relative(sourceRoot, link));
        if (!target.startsWith(destinationRoot + path.sep))
          throw Error("Resource link points outside the installation");
        if (path.isAbsolute(link)) {
          await fs.unlink(file);
          await fs.symlink(path.relative(directory, target), file);
        }
        links.push(file);
      }
    }
  }
  for (const name of resourceNames) {
    const directory = path.join(destinationRoot, name);
    if (await fs.stat(directory).catch(() => null)) await visit(directory);
  }
  for (const file of links) {
    const resolved = await fs.realpath(file);
    if (!resolved.startsWith(destinationRoot + path.sep))
      throw Error("Resource link resolves outside the installation");
  }
}
exports.resourceNames = resourceNames;
exports.normalizeResourceLinks = normalizeResourceLinks;
