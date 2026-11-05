import path from "node:path";

export function trustedFrame(
  sender: number,
  owner: number,
  address: string,
  main: boolean,
): boolean {
  if (sender !== owner || !main) return false;
  try {
    const url = new URL(address);
    return (
      url.protocol === "momo:" &&
      url.host === "app" &&
      !url.username &&
      !url.password
    );
  } catch {
    return false;
  }
}

export function assetPath(root: string, address: string): string {
  const url = new URL(address);
  if (!trustedFrame(1, 1, address, true))
    throw new Error("Invalid asset origin");
  const name = decodeURIComponent(url.pathname);
  if (
    name.includes("\0") ||
    name.includes("\\") ||
    name.split("/").includes("..")
  )
    throw new Error("Invalid asset path");
  const file = path.resolve(root, "." + (name === "/" ? "/index.html" : name));
  if (!file.startsWith(path.resolve(root) + path.sep))
    throw new Error("Invalid asset path");
  return file;
}

export function contentSecurityPolicy(baseUrl: string): string {
  return `default-src 'self'; script-src 'self' blob: 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: ${baseUrl}; media-src 'self' blob: data: ${baseUrl}; font-src 'self' data:; connect-src 'self' blob: ${baseUrl} ${baseUrl.replace("http:", "ws:")}; worker-src 'self' blob:; object-src 'none'; base-uri 'self'; frame-src 'none'; frame-ancestors 'none'`;
}

export function mediaPermission(
  permission: string,
  trustedMainFrame: boolean,
  mediaTypes: readonly string[],
) {
  if (!trustedMainFrame) return undefined;
  if (permission === "display-capture") return "display";
  if (
    permission === "media" &&
    mediaTypes.length > 0 &&
    mediaTypes.every((type) => type === "audio")
  )
    return "microphone";
  return undefined;
}
