import type { AiTask, AnalysisMode } from "@aifos/ai-contracts";

const VALID_TASKS = new Set<AiTask>([
  "cash_scenario",
  "overdue_exposure",
  "payment_priority",
  "profit_movement",
  "supplier_trend",
]);

export interface TenantPolicy {
  tenantId: string;
  active: boolean;
  allowedTasks: AiTask[];
  providerByMode: Record<AnalysisMode, string | null>;
  maximumContextRecords: number;
  maximumDailyRequests: number;
  maximumDailyCostMicros: number;
  maximumCostPerRequestMicros: number;
  providerPayloadRetention: "none";
}

export interface TenantPolicyStore {
  get(tenantId: string): Promise<TenantPolicy | undefined>;
}

export class StaticTenantPolicyStore implements TenantPolicyStore {
  readonly #policies: ReadonlyMap<string, TenantPolicy>;

  constructor(policies: TenantPolicy[]) {
    for (const policy of policies) validatePolicy(policy);
    if (
      new Set(policies.map((policy) => policy.tenantId)).size !==
      policies.length
    ) {
      throw new Error("Tenant policy identifiers must be unique.");
    }
    this.#policies = new Map(
      policies.map((policy) => [policy.tenantId, policy]),
    );
  }

  async get(tenantId: string): Promise<TenantPolicy | undefined> {
    return this.#policies.get(tenantId);
  }
}

function validatePolicy(policy: TenantPolicy): void {
  const providers = Object.values(policy.providerByMode);
  if (
    !/^[a-zA-Z0-9][a-zA-Z0-9._:-]{0,127}$/u.test(policy.tenantId) ||
    policy.allowedTasks.length === 0 ||
    new Set(policy.allowedTasks).size !== policy.allowedTasks.length ||
    policy.allowedTasks.some((task) => !VALID_TASKS.has(task)) ||
    providers.some(
      (provider) =>
        provider !== null && !/^[a-z][a-z0-9-]{0,63}$/u.test(provider),
    ) ||
    !Number.isSafeInteger(policy.maximumContextRecords) ||
    policy.maximumContextRecords < 1 ||
    policy.maximumContextRecords > 5_000 ||
    !Number.isSafeInteger(policy.maximumDailyRequests) ||
    policy.maximumDailyRequests < 1 ||
    !Number.isSafeInteger(policy.maximumDailyCostMicros) ||
    policy.maximumDailyCostMicros < 0 ||
    !Number.isSafeInteger(policy.maximumCostPerRequestMicros) ||
    policy.maximumCostPerRequestMicros < 0 ||
    policy.maximumCostPerRequestMicros > policy.maximumDailyCostMicros ||
    policy.providerPayloadRetention !== "none"
  ) {
    throw new Error("Tenant policy is invalid.");
  }
}
