import { generateKeyPairSync, sign } from "node:crypto";
import type { AddressInfo } from "node:net";
import type {
  AiAnalysisOutputV1,
  GatewayAnalysisRequestV1,
} from "@aifos/ai-contracts";
import { describe, expect, it } from "vitest";
import { TenantAuthenticator } from "./auth.js";
import { InMemoryGatewayAuditSink } from "./audit.js";
import { REQUIRED_DISCLAIMER } from "./contracts.js";
import { GatewayError } from "./errors.js";
import { CloudAnalysisGateway } from "./gateway.js";
import { createGatewayHttpServer } from "./http.js";
import { InMemoryMeteringSink } from "./metering.js";
import { StaticTenantPolicyStore, type TenantPolicy } from "./policy.js";
import {
  ProviderRegistry,
  type ProviderAdapter,
  type ProviderResult,
} from "./provider.js";

const NOW_MS = 1_800_000_000_000;
const NOW_SECONDS = Math.floor(NOW_MS / 1000);
const TENANT = "tenant-alpha";
const OTHER_TENANT = "tenant-beta";
const REQUEST_ID = "10000000-0000-4000-8000-000000000001";
const CONSENT_ID = "20000000-0000-4000-8000-000000000001";

const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const publicKeyPem = publicKey
  .export({ format: "pem", type: "spki" })
  .toString();

function token(overrides: Partial<Record<string, unknown>> = {}): string {
  const header = Buffer.from(
    JSON.stringify({ alg: "EdDSA", typ: "AIFOS-TENANT", v: 1 }),
  ).toString("base64url");
  const payload = Buffer.from(
    JSON.stringify({
      iss: "https://identity.aifos.test",
      aud: "aifos-cloud-gateway",
      sub: "user-1",
      tenantId: TENANT,
      roles: ["ai:invoke"],
      iat: NOW_SECONDS - 10,
      exp: NOW_SECONDS + 300,
      jti: "session-1",
      ...overrides,
    }),
  ).toString("base64url");
  const signature = sign(
    null,
    Buffer.from(`${header}.${payload}`, "ascii"),
    privateKey,
  ).toString("base64url");
  return `Bearer ${header}.${payload}.${signature}`;
}

function request(
  overrides: Partial<GatewayAnalysisRequestV1> = {},
): GatewayAnalysisRequestV1 {
  return {
    schemaVersion: 1,
    requestId: REQUEST_ID,
    tenantId: TENANT,
    workspaceReference: "workspace-pseudonym-1",
    task: "supplier_trend",
    mode: "standard",
    question: "Which approved supplier movement needs review?",
    consent: {
      receiptId: CONSENT_ID,
      grantedAtMs: NOW_MS - 1_000,
      categories: ["approved_invoice"],
      approximateRecordCount: 1,
    },
    context: {
      records: [
        {
          sourceId: "invoice-source-1",
          category: "approved_invoice",
          fields: { currency: "INR", amountMinor: 125_00 },
        },
      ],
    },
    ...overrides,
  };
}

function validOutput(): AiAnalysisOutputV1 {
  return {
    schemaVersion: 1,
    narrative:
      "The selected approved record shows a movement that merits review.",
    citations: [
      {
        claim: "One selected record supports this observation.",
        sourceId: "invoice-source-1",
        sourceField: "amountMinor",
        sourceValue: 125_00,
      },
    ],
    assumptions: ["Only the explicitly selected record was considered."],
    missingData: ["Payment clearance data was not provided."],
    proposedActions: ["Review the cited invoice before making a decision."],
    confidence: "medium",
    disclaimer: REQUIRED_DISCLAIMER,
  };
}

class QueueProvider implements ProviderAdapter {
  readonly providerId: string;
  readonly #results: Array<ProviderResult | Error>;
  calls = 0;

  constructor(providerId: string, results: Array<ProviderResult | Error>) {
    this.providerId = providerId;
    this.#results = results;
  }

  estimateMaximumCostMicros(): number {
    return 2_000;
  }

  async generate(): Promise<ProviderResult> {
    const result = this.#results[this.calls];
    this.calls += 1;
    if (!result) throw new Error("Test provider result queue exhausted.");
    if (result instanceof Error) throw result;
    return result;
  }
}

function providerResult(
  output: unknown = validOutput(),
  costMicros = 1_250,
): ProviderResult {
  return {
    modelId: "configured-model-alias",
    output,
    cost: { currency: "USD", inputUnits: 100, outputUnits: 40, costMicros },
  };
}

function policy(overrides: Partial<TenantPolicy> = {}): TenantPolicy {
  return {
    tenantId: TENANT,
    active: true,
    allowedTasks: [
      "cash_scenario",
      "overdue_exposure",
      "payment_priority",
      "profit_movement",
      "supplier_trend",
    ],
    providerByMode: { economy: "provider-a", standard: "provider-a" },
    maximumContextRecords: 100,
    maximumDailyRequests: 20,
    maximumDailyCostMicros: 100_000,
    maximumCostPerRequestMicros: 10_000,
    providerPayloadRetention: "none",
    ...overrides,
  };
}

function gateway(input?: {
  policies?: TenantPolicy[];
  providers?: ProviderAdapter[];
  metering?: InMemoryMeteringSink;
}): {
  gateway: CloudAnalysisGateway;
  metering: InMemoryMeteringSink;
  audit: InMemoryGatewayAuditSink;
} {
  const metering = input?.metering ?? new InMemoryMeteringSink();
  const audit = new InMemoryGatewayAuditSink();
  return {
    gateway: new CloudAnalysisGateway({
      authenticator: new TenantAuthenticator({
        publicKeyPem,
        issuer: "https://identity.aifos.test",
        audience: "aifos-cloud-gateway",
        nowSeconds: () => NOW_SECONDS,
      }),
      policies: new StaticTenantPolicyStore(input?.policies ?? [policy()]),
      providers: new ProviderRegistry(
        input?.providers ?? [
          new QueueProvider("provider-a", [providerResult()]),
        ],
      ),
      metering,
      audit,
      nowMs: () => NOW_MS,
    }),
    metering,
    audit,
  };
}

async function expectCode(
  promise: Promise<unknown>,
  code: string,
): Promise<void> {
  await expect(promise).rejects.toMatchObject({ code });
}

describe("WP-09 cloud gateway", () => {
  it("authenticates, applies tenant policy, validates output, and meters without finance payloads", async () => {
    const provider = new QueueProvider("provider-a", [providerResult()]);
    const context = gateway({ providers: [provider] });
    const response = await context.gateway.analyze(token(), request());

    expect(response.output.citations[0]?.sourceId).toBe("invoice-source-1");
    expect(response.execution).toEqual({
      providerId: "provider-a",
      modelId: "configured-model-alias",
      attempts: 1,
      cost: {
        currency: "USD",
        inputUnits: 100,
        outputUnits: 40,
        costMicros: 1_250,
      },
    });
    const records = context.metering.recordsForTenant(TENANT);
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({
      tenantId: TENANT,
      requestId: REQUEST_ID,
      status: "succeeded",
      costMicros: 1_250,
    });
    expect(JSON.stringify(records)).not.toContain("Which approved supplier");
    expect(JSON.stringify(records)).not.toContain("invoice-source-1");
    expect(context.audit.events()).toEqual([
      {
        eventType: "analysis_completed",
        tenantId: TENANT,
        subject: "user-1",
        requestId: REQUEST_ID,
        outcomeCode: "succeeded",
        occurredAtMs: NOW_MS,
      },
    ]);
  });

  it("rejects tampered, expired, unauthorized, and cross-tenant sessions", async () => {
    const context = gateway();
    const valid = token();
    const tampered = `${valid.slice(0, -1)}${valid.endsWith("A") ? "B" : "A"}`;
    await expectCode(
      context.gateway.analyze(tampered, request()),
      "authentication_failed",
    );
    await expectCode(
      context.gateway.analyze(token({ exp: NOW_SECONDS }), request()),
      "authentication_failed",
    );
    await expectCode(
      context.gateway.analyze(token({ roles: ["viewer"] }), request()),
      "authorization_failed",
    );
    await expectCode(
      context.gateway.analyze(token({ tenantId: OTHER_TENANT }), request()),
      "authorization_failed",
    );
    expect(context.audit.events().map((event) => event.eventType)).toEqual([
      "authentication_failed",
      "authentication_failed",
      "analysis_rejected",
      "analysis_rejected",
    ]);
    expect(JSON.stringify(context.audit.events())).not.toContain(
      "approved supplier",
    );
  });

  it("selects providers entirely from tenant policy", async () => {
    const providerA = new QueueProvider("provider-a", [providerResult()]);
    const providerB = new QueueProvider("provider-b", [providerResult()]);
    const context = gateway({
      policies: [
        policy({
          providerByMode: { economy: "provider-a", standard: "provider-b" },
        }),
      ],
      providers: [providerA, providerB],
    });
    const response = await context.gateway.analyze(token(), request());
    expect(response.execution.providerId).toBe("provider-b");
    expect(providerA.calls).toBe(0);
    expect(providerB.calls).toBe(1);
  });

  it("retries one invalid structured response and aggregates cost metadata", async () => {
    const invalid = { ...validOutput(), citations: [] };
    const provider = new QueueProvider("provider-a", [
      providerResult(invalid, 500),
      providerResult(validOutput(), 750),
    ]);
    const context = gateway({ providers: [provider] });
    const response = await context.gateway.analyze(token(), request());
    expect(response.execution.attempts).toBe(2);
    expect(response.execution.cost).toEqual({
      currency: "USD",
      inputUnits: 200,
      outputUnits: 80,
      costMicros: 1_250,
    });
    expect(context.metering.recordsForTenant(TENANT)[0]?.attempts).toBe(2);
  });

  it("fails closed after two invalid outputs and never accepts an unknown citation", async () => {
    const invalid = {
      ...validOutput(),
      citations: [
        {
          claim: "Unsupported claim.",
          sourceId: "another-tenant-source",
          sourceField: "amountMinor",
          sourceValue: 125_00,
        },
      ],
    };
    const context = gateway({
      providers: [
        new QueueProvider("provider-a", [
          providerResult(invalid),
          providerResult(invalid),
        ]),
      ],
    });
    await expectCode(
      context.gateway.analyze(token(), request()),
      "invalid_provider_output",
    );
    expect(context.metering.recordsForTenant(TENANT)[0]?.status).toBe(
      "invalid_output",
    );
  });

  it("rejects mismatched frozen values and uncited numeric narrative claims", async () => {
    const wrongValue = {
      ...validOutput(),
      citations: [
        {
          claim: "The selected amount is 99900 minor units.",
          sourceId: "invoice-source-1",
          sourceField: "amountMinor",
          sourceValue: 999_00,
        },
      ],
    };
    const uncitedNumber = {
      ...validOutput(),
      narrative: "The selected record increased by 25 percent.",
    };
    for (const invalid of [wrongValue, uncitedNumber]) {
      const context = gateway({
        providers: [
          new QueueProvider("provider-a", [
            providerResult(invalid),
            providerResult(invalid),
          ]),
        ],
      });
      await expectCode(
        context.gateway.analyze(token(), request()),
        "invalid_provider_output",
      );
    }
  });

  it("enforces task, record, per-request, daily, and duplicate-request policy", async () => {
    const deniedTask = gateway({
      policies: [policy({ allowedTasks: ["profit_movement"] })],
    });
    await expectCode(
      deniedTask.gateway.analyze(token(), request()),
      "policy_denied",
    );

    const deniedRecords = gateway({
      policies: [policy({ maximumContextRecords: 1 })],
    });
    const secondRecord = {
      ...request().context.records[0]!,
      sourceId: "invoice-source-2",
    };
    await expectCode(
      deniedRecords.gateway.analyze(
        token(),
        request({
          consent: { ...request().consent, approximateRecordCount: 2 },
          context: { records: [...request().context.records, secondRecord] },
        }),
      ),
      "policy_denied",
    );

    const deniedCost = gateway({
      policies: [policy({ maximumCostPerRequestMicros: 1_000 })],
    });
    await expectCode(
      deniedCost.gateway.analyze(token(), request()),
      "budget_exceeded",
    );

    const provider = new QueueProvider("provider-a", [providerResult()]);
    const daily = gateway({
      policies: [policy({ maximumDailyRequests: 1 })],
      providers: [provider],
    });
    await daily.gateway.analyze(token(), request());
    await expectCode(
      daily.gateway.analyze(
        token(),
        request({ requestId: "10000000-0000-4000-8000-000000000002" }),
      ),
      "budget_exceeded",
    );

    const duplicateProvider = new QueueProvider("provider-a", [
      providerResult(),
    ]);
    const duplicate = gateway({ providers: [duplicateProvider] });
    await duplicate.gateway.analyze(token(), request());
    await expectCode(
      duplicate.gateway.analyze(token(), request()),
      "duplicate_request",
    );
    expect(duplicateProvider.calls).toBe(1);
  });

  it("rejects unknown request fields, consent mismatches, and unsafe policy configuration", async () => {
    const context = gateway();
    await expectCode(
      context.gateway.analyze(token(), {
        ...request(),
        providerId: "attacker-choice",
      }),
      "invalid_request",
    );
    await expectCode(
      context.gateway.analyze(token(), {
        ...request(),
        consent: { ...request().consent, approximateRecordCount: 99 },
      }),
      "invalid_request",
    );
    expect(
      () =>
        new StaticTenantPolicyStore([
          policy({ providerPayloadRetention: "raw" as "none" }),
        ]),
    ).toThrow("Tenant policy is invalid");
  });

  it("uses safe public errors and does not expose provider failure text", async () => {
    const context = gateway({
      providers: [
        new QueueProvider("provider-a", [
          new Error("provider secret and upstream response must not escape"),
        ]),
      ],
    });
    const failure = await context.gateway
      .analyze(token(), request())
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(GatewayError);
    expect(failure).toMatchObject({
      code: "provider_unavailable",
      message: "No approved cloud provider is configured.",
    });
    expect(String(failure)).not.toContain("provider secret");
    expect(context.metering.recordsForTenant(TENANT)[0]?.status).toBe(
      "provider_error",
    );
  });

  it("requires explicitly trusted secure transport and emits no-store JSON errors", async () => {
    const context = gateway();
    const server = createGatewayHttpServer(context.gateway, {
      requireSecureTransport: true,
      trustForwardedProto: true,
    });
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    try {
      const address = server.address() as AddressInfo;
      const base = `http://127.0.0.1:${address.port}`;
      const health = await fetch(`${base}/health`);
      expect(await health.json()).toEqual({
        status: "ok",
        providerConfigured: true,
      });

      const insecure = await fetch(`${base}/v1/analysis`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(request()),
      });
      expect(insecure.status).toBe(400);
      expect(insecure.headers.get("cache-control")).toBe("no-store");
      expect(await insecure.json()).toMatchObject({
        error: { code: "secure_transport_required" },
      });

      const unauthenticated = await fetch(`${base}/v1/analysis`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-forwarded-proto": "https",
        },
        body: JSON.stringify(request()),
      });
      expect(unauthenticated.status).toBe(401);
      expect(await unauthenticated.json()).toEqual({
        error: {
          code: "authentication_failed",
          message: "Authentication failed.",
        },
      });
    } finally {
      await new Promise<void>((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      );
    }
  });
});
