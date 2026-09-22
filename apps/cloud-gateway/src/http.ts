import { createServer } from "node:http";
import type { IncomingMessage, Server, ServerResponse } from "node:http";
import { TLSSocket } from "node:tls";
import type { CloudAnalysisGateway } from "./gateway.js";
import { GatewayError } from "./errors.js";

const MAX_BODY_BYTES = 256 * 1024;

interface HttpOptions {
  requireSecureTransport: boolean;
  trustForwardedProto?: boolean;
}

export function createGatewayHttpServer(
  gateway: CloudAnalysisGateway,
  options: HttpOptions = { requireSecureTransport: true },
): Server {
  const server = createServer((request, response) => {
    void route(gateway, options, request, response);
  });
  server.headersTimeout = 10_000;
  server.requestTimeout = 15_000;
  server.keepAliveTimeout = 5_000;
  server.maxHeadersCount = 64;
  return server;
}

async function route(
  gateway: CloudAnalysisGateway,
  options: HttpOptions,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  try {
    if (request.method === "GET" && request.url === "/health") {
      writeJson(response, 200, {
        status: "ok",
        providerConfigured: gateway.configuredProviderCount > 0,
      });
      return;
    }
    if (request.method !== "POST" || request.url !== "/v1/analysis") {
      writeJson(response, 404, {
        error: { code: "not_found", message: "Not found." },
      });
      return;
    }
    const secureTransport =
      request.socket instanceof TLSSocket ||
      (options.trustForwardedProto === true &&
        request.headers["x-forwarded-proto"] === "https");
    if (options.requireSecureTransport && !secureTransport) {
      throw new GatewayError("secure_transport_required");
    }
    if (
      !String(request.headers["content-type"] ?? "").startsWith(
        "application/json",
      )
    ) {
      throw new GatewayError("invalid_request");
    }
    const body = await readJsonBody(request);
    const result = await gateway.analyze(request.headers.authorization, body);
    writeJson(response, 200, result);
  } catch (error) {
    const safe =
      error instanceof GatewayError
        ? error
        : new GatewayError("provider_unavailable");
    writeJson(response, safe.status, {
      error: { code: safe.code, message: safe.message },
    });
  }
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += bytes.length;
    if (size > MAX_BODY_BYTES) throw new GatewayError("payload_too_large");
    chunks.push(bytes);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new GatewayError("invalid_request");
  }
}

function writeJson(
  response: ServerResponse,
  status: number,
  value: unknown,
): void {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "cache-control": "no-store",
    "content-length": Buffer.byteLength(body),
    "content-type": "application/json; charset=utf-8",
    "x-content-type-options": "nosniff",
  });
  response.end(body);
}
