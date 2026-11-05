import { net, protocol } from "electron";
import { realpath } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { assetPath, contentSecurityPolicy } from "./security.ts";

export function registerProtocol(
  rendererRoot: string,
  connection: () => string,
) {
  protocol.handle("momo", async (request) => {
    if (request.method !== "GET" && request.method !== "HEAD")
      return new Response(null, { status: 405 });
    try {
      const root = await realpath(rendererRoot);
      const file = await realpath(assetPath(root, request.url));
      if (!file.startsWith(root + path.sep))
        return new Response(null, { status: 403 });
      const result = await net.fetch(pathToFileURL(file).toString());
      const headers = new Headers(result.headers);
      headers.set(
        "Content-Security-Policy",
        contentSecurityPolicy(connection()),
      );
      headers.set("X-Content-Type-Options", "nosniff");
      return new Response(request.method === "HEAD" ? null : result.body, {
        status: result.status,
        headers,
      });
    } catch {
      return new Response(null, { status: 404 });
    }
  });
}
