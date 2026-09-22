import { readFileSync } from "node:fs";
import process from "node:process";
import { InMemoryGatewayAuditSink } from "./audit.js";
import { TenantAuthenticator } from "./auth.js";
import { CloudAnalysisGateway } from "./gateway.js";
import { createGatewayHttpServer } from "./http.js";
import { InMemoryMeteringSink } from "./metering.js";
import { StaticTenantPolicyStore } from "./policy.js";
import { ProviderRegistry } from "./provider.js";

const publicKeyPath = requiredEnvironment("AIFOS_GATEWAY_PUBLIC_KEY_FILE");
const issuer = requiredEnvironment("AIFOS_GATEWAY_TOKEN_ISSUER");
const audience = requiredEnvironment("AIFOS_GATEWAY_TOKEN_AUDIENCE");
const port = parsePort(process.env.AIFOS_GATEWAY_PORT ?? "8080");
const host = parseHost(process.env.AIFOS_GATEWAY_HOST ?? "127.0.0.1");

const gateway = new CloudAnalysisGateway({
  authenticator: new TenantAuthenticator({
    publicKeyPem: readFileSync(publicKeyPath, "utf8"),
    issuer,
    audience,
  }),
  policies: new StaticTenantPolicyStore([]),
  providers: new ProviderRegistry([]),
  metering: new InMemoryMeteringSink(),
  audit: new InMemoryGatewayAuditSink(),
});

const server = createGatewayHttpServer(gateway, {
  requireSecureTransport: true,
  trustForwardedProto: process.env.AIFOS_GATEWAY_TRUST_PROXY === "true",
});

server.listen(port, host, () => {
  process.stdout.write(
    `AIFOS gateway boundary listening on ${host}:${port}; live providers: 0\n`,
  );
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    server.close(() => process.exit(0));
  });
}

function requiredEnvironment(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`Required server setting ${name} is missing.`);
  return value;
}

function parsePort(value: string): number {
  const portNumber = Number(value);
  if (
    !Number.isSafeInteger(portNumber) ||
    portNumber < 1 ||
    portNumber > 65_535
  ) {
    throw new Error(
      "AIFOS_GATEWAY_PORT must be an integer from 1 through 65535.",
    );
  }
  return portNumber;
}

function parseHost(value: string): string {
  if (!["127.0.0.1", "0.0.0.0", "::"].includes(value)) {
    throw new Error("AIFOS_GATEWAY_HOST must be 127.0.0.1, 0.0.0.0, or ::.");
  }
  return value;
}
