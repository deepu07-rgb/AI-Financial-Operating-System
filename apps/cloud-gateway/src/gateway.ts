import type {
  GatewayAnalysisResponseV1,
  ProviderCostMetadata,
} from "@aifos/ai-contracts";
import type { GatewayAuditEvent, GatewayAuditSink } from "./audit.js";
import type { TenantAuthenticator, TenantClaims } from "./auth.js";
import {
  parseAnalysisOutput,
  parseAnalysisRequest,
  parseCostMetadata,
} from "./contracts.js";
import { GatewayError } from "./errors.js";
import type { MeteringSink, MeteringStatus } from "./metering.js";
import type { TenantPolicyStore } from "./policy.js";
import type { ProviderRegistry } from "./provider.js";

interface GatewayOptions {
  authenticator: TenantAuthenticator;
  policies: TenantPolicyStore;
  providers: ProviderRegistry;
  metering: MeteringSink;
  audit: GatewayAuditSink;
  nowMs?: () => number;
}

export class CloudAnalysisGateway {
  readonly #authenticator: TenantAuthenticator;
  readonly #policies: TenantPolicyStore;
  readonly #providers: ProviderRegistry;
  readonly #metering: MeteringSink;
  readonly #audit: GatewayAuditSink;
  readonly #nowMs: () => number;

  constructor(options: GatewayOptions) {
    this.#authenticator = options.authenticator;
    this.#policies = options.policies;
    this.#providers = options.providers;
    this.#metering = options.metering;
    this.#audit = options.audit;
    this.#nowMs = options.nowMs ?? Date.now;
  }

  get configuredProviderCount(): number {
    return this.#providers.configuredProviderCount;
  }

  async analyze(
    authorizationHeader: string | undefined,
    body: unknown,
  ): Promise<GatewayAnalysisResponseV1> {
    let claims: TenantClaims;
    try {
      claims = this.#authenticator.authenticate(authorizationHeader);
    } catch {
      await this.#appendAudit({
        eventType: "authentication_failed",
        tenantId: null,
        subject: null,
        requestId: null,
        outcomeCode: "authentication_failed",
        occurredAtMs: this.#nowMs(),
      });
      throw new GatewayError("authentication_failed");
    }
    try {
      const response = await this.#analyzeForClaims(claims, body);
      await this.#appendAudit({
        eventType: "analysis_completed",
        tenantId: claims.tenantId,
        subject: claims.subject,
        requestId: response.requestId,
        outcomeCode: "succeeded",
        occurredAtMs: this.#nowMs(),
      });
      return response;
    } catch (error) {
      const safe =
        error instanceof GatewayError
          ? error
          : new GatewayError("provider_unavailable");
      await this.#appendAudit({
        eventType: "analysis_rejected",
        tenantId: claims.tenantId,
        subject: claims.subject,
        requestId: safeRequestId(body),
        outcomeCode: safe.code,
        occurredAtMs: this.#nowMs(),
      });
      throw safe;
    }
  }

  async #appendAudit(event: GatewayAuditEvent): Promise<void> {
    try {
      await this.#audit.append(event);
    } catch {
      throw new GatewayError("provider_unavailable");
    }
  }

  async #analyzeForClaims(
    claims: TenantClaims,
    body: unknown,
  ): Promise<GatewayAnalysisResponseV1> {
    if (!claims.roles.includes("ai:invoke")) {
      throw new GatewayError("authorization_failed");
    }
    const request = parseAnalysisRequest(body);
    if (claims.tenantId !== request.tenantId) {
      throw new GatewayError("authorization_failed");
    }
    const policy = await this.#policies.get(request.tenantId);
    if (
      !policy?.active ||
      policy.providerPayloadRetention !== "none" ||
      !policy.allowedTasks.includes(request.task) ||
      request.context.records.length > policy.maximumContextRecords
    ) {
      throw new GatewayError("policy_denied");
    }
    const providerId = policy.providerByMode[request.mode];
    if (!providerId) throw new GatewayError("provider_unavailable");
    const provider = this.#providers.get(providerId);
    if (!provider) throw new GatewayError("provider_unavailable");

    const estimatedCost = provider.estimateMaximumCostMicros(request);
    if (
      !Number.isSafeInteger(estimatedCost) ||
      estimatedCost < 0 ||
      estimatedCost > policy.maximumCostPerRequestMicros
    ) {
      throw new GatewayError("budget_exceeded");
    }
    const now = this.#nowMs();
    if (
      request.consent.grantedAtMs > now + 30_000 ||
      now - request.consent.grantedAtMs > 3_600_000
    ) {
      throw new GatewayError("policy_denied");
    }
    const projectedCost = estimatedCost * 2;
    if (!Number.isSafeInteger(projectedCost)) {
      throw new GatewayError("budget_exceeded");
    }
    const reservation = await this.#metering.reserve({
      tenantId: request.tenantId,
      requestId: request.requestId,
      occurredAtMs: now,
      projectedCostMicros: projectedCost,
      maximumDailyRequests: policy.maximumDailyRequests,
      maximumDailyCostMicros: policy.maximumDailyCostMicros,
    });
    if (reservation === "duplicate") {
      throw new GatewayError("duplicate_request");
    }
    if (reservation === "budget_exceeded")
      throw new GatewayError("budget_exceeded");

    let attempts = 0;
    let totalCost: ProviderCostMetadata = {
      currency: "USD",
      inputUnits: 0,
      outputUnits: 0,
      costMicros: 0,
    };
    let modelId: string | null = null;
    let finalStatus: MeteringStatus = "provider_error";
    let response: GatewayAnalysisResponseV1 | undefined;
    let failure: GatewayError | undefined;
    try {
      while (attempts < 2) {
        attempts += 1;
        const result = await provider.generate(request);
        if (!isProviderModelId(result.modelId)) {
          throw new GatewayError("invalid_provider_output");
        }
        modelId = result.modelId;
        const cost = parseCostMetadata(result.cost);
        totalCost = addCost(totalCost, cost);
        if (
          totalCost.costMicros > policy.maximumCostPerRequestMicros ||
          totalCost.costMicros > estimatedCost * attempts
        ) {
          throw new GatewayError("budget_exceeded");
        }
        try {
          const output = parseAnalysisOutput(
            result.output,
            request.context.records,
          );
          finalStatus = "succeeded";
          response = {
            schemaVersion: 1,
            requestId: request.requestId,
            output,
            execution: {
              providerId,
              modelId: result.modelId,
              attempts,
              cost: totalCost,
            },
          };
          break;
        } catch (error) {
          if (
            !(error instanceof GatewayError) ||
            error.code !== "invalid_provider_output"
          ) {
            throw error;
          }
          finalStatus = "invalid_output";
          if (attempts === 2) throw error;
        }
      }
      if (!response) throw new GatewayError("invalid_provider_output");
    } catch (error) {
      failure =
        error instanceof GatewayError
          ? error
          : new GatewayError("provider_unavailable");
    }
    try {
      await this.#metering.finalize({
        tenantId: request.tenantId,
        requestId: request.requestId,
        providerId,
        modelId,
        attempts,
        inputUnits: totalCost.inputUnits,
        outputUnits: totalCost.outputUnits,
        costMicros: totalCost.costMicros,
        currency: "USD",
        status: finalStatus,
        occurredAtMs: now,
      });
    } catch {
      throw new GatewayError("provider_unavailable");
    }
    if (failure) throw failure;
    if (!response) throw new GatewayError("invalid_provider_output");
    return response;
  }
}

function safeRequestId(value: unknown): string | null {
  if (
    typeof value === "object" &&
    value !== null &&
    "requestId" in value &&
    typeof value.requestId === "string" &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u.test(
      value.requestId,
    )
  ) {
    return value.requestId;
  }
  return null;
}

function isProviderModelId(value: unknown): value is string {
  return (
    typeof value === "string" &&
    /^[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,127}$/u.test(value)
  );
}

function addCost(
  current: ProviderCostMetadata,
  next: ProviderCostMetadata,
): ProviderCostMetadata {
  const inputUnits = current.inputUnits + next.inputUnits;
  const outputUnits = current.outputUnits + next.outputUnits;
  const costMicros = current.costMicros + next.costMicros;
  if (
    !Number.isSafeInteger(inputUnits) ||
    !Number.isSafeInteger(outputUnits) ||
    !Number.isSafeInteger(costMicros)
  ) {
    throw new GatewayError("invalid_provider_output");
  }
  return { currency: "USD", inputUnits, outputUnits, costMicros };
}
