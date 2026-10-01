// SPDX-License-Identifier: Apache-2.0

import { request } from "node:http";

const MAX_RESPONSE_BYTES = 1024 * 1024;

export function loopbackJson(port, pathname, timeoutMs) {
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("invalid loopback port");
  if (!pathname.startsWith("/") || pathname.startsWith("//")) throw new Error("invalid loopback path");

  return new Promise((resolve, reject) => {
    const requestHandle = request({
      hostname: "127.0.0.1",
      port,
      path: pathname,
      method: "GET",
      timeout: timeoutMs,
    }, (response) => {
      let body = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => {
        body += chunk;
        if (body.length > MAX_RESPONSE_BYTES) requestHandle.destroy(new Error("loopback response exceeded 1 MiB"));
      });
      response.on("end", () => {
        const statusCode = response.statusCode ?? 0;
        resolve({ ok: statusCode >= 200 && statusCode < 300, json: body ? JSON.parse(body) : null });
      });
    });
    requestHandle.on("timeout", () => requestHandle.destroy(new Error("loopback request timed out")));
    requestHandle.on("error", reject);
    requestHandle.end();
  });
}
