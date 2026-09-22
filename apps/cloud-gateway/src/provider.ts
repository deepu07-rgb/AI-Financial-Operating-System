import type {
  GatewayAnalysisRequestV1,
  ProviderCostMetadata,
} from "@aifos/ai-contracts";

export interface ProviderResult {
  modelId: string;
  output: unknown;
  cost: ProviderCostMetadata;
}

export interface ProviderAdapter {
  readonly providerId: string;
  estimateMaximumCostMicros(request: GatewayAnalysisRequestV1): number;
  generate(request: GatewayAnalysisRequestV1): Promise<ProviderResult>;
}

export class ProviderRegistry {
  readonly #providers: ReadonlyMap<string, ProviderAdapter>;

  constructor(providers: ProviderAdapter[]) {
    const entries = providers.map(
      (provider) => [provider.providerId, provider] as const,
    );
    if (
      new Set(entries.map(([id]) => id)).size !== entries.length ||
      entries.some(([id]) => !/^[a-z][a-z0-9-]{0,63}$/u.test(id))
    ) {
      throw new Error("Provider identifiers must be valid and unique.");
    }
    this.#providers = new Map(entries);
  }

  get(providerId: string): ProviderAdapter | undefined {
    return this.#providers.get(providerId);
  }

  get configuredProviderCount(): number {
    return this.#providers.size;
  }
}
