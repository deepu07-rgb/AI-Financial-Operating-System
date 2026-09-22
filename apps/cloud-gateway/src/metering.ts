export type MeteringStatus = "invalid_output" | "provider_error" | "succeeded";

export interface UsageTotals {
  requestCount: number;
  costMicros: number;
}

export type ReservationResult = "budget_exceeded" | "duplicate" | "reserved";

export interface MeteringRecord {
  tenantId: string;
  requestId: string;
  providerId: string;
  modelId: string | null;
  attempts: number;
  inputUnits: number;
  outputUnits: number;
  costMicros: number;
  currency: "USD";
  status: MeteringStatus;
  occurredAtMs: number;
}

export interface MeteringSink {
  usageForUtcDay(tenantId: string, occurredAtMs: number): Promise<UsageTotals>;
  reserve(input: {
    tenantId: string;
    requestId: string;
    occurredAtMs: number;
    projectedCostMicros: number;
    maximumDailyRequests: number;
    maximumDailyCostMicros: number;
  }): Promise<ReservationResult>;
  finalize(record: MeteringRecord): Promise<void>;
}

export class InMemoryMeteringSink implements MeteringSink {
  readonly #reserved = new Set<string>();
  readonly #activeReservations = new Map<
    string,
    { tenantId: string; occurredAtMs: number; projectedCostMicros: number }
  >();
  readonly #records: MeteringRecord[] = [];

  async usageForUtcDay(
    tenantId: string,
    occurredAtMs: number,
  ): Promise<UsageTotals> {
    const date = new Date(occurredAtMs).toISOString().slice(0, 10);
    const records = this.#records.filter(
      (record) =>
        record.tenantId === tenantId &&
        new Date(record.occurredAtMs).toISOString().startsWith(date),
    );
    const reservations = [...this.#activeReservations.values()].filter(
      (reservation) =>
        reservation.tenantId === tenantId &&
        new Date(reservation.occurredAtMs).toISOString().startsWith(date),
    );
    return {
      requestCount: records.length + reservations.length,
      costMicros:
        records.reduce((sum, record) => sum + record.costMicros, 0) +
        reservations.reduce(
          (sum, record) => sum + record.projectedCostMicros,
          0,
        ),
    };
  }

  async reserve(input: {
    tenantId: string;
    requestId: string;
    occurredAtMs: number;
    projectedCostMicros: number;
    maximumDailyRequests: number;
    maximumDailyCostMicros: number;
  }): Promise<ReservationResult> {
    const key = `${input.tenantId}\0${input.requestId}`;
    if (this.#reserved.has(key)) return "duplicate";
    const usage = await this.usageForUtcDay(input.tenantId, input.occurredAtMs);
    if (
      usage.requestCount >= input.maximumDailyRequests ||
      usage.costMicros + input.projectedCostMicros >
        input.maximumDailyCostMicros
    ) {
      return "budget_exceeded";
    }
    this.#reserved.add(key);
    this.#activeReservations.set(key, {
      tenantId: input.tenantId,
      occurredAtMs: input.occurredAtMs,
      projectedCostMicros: input.projectedCostMicros,
    });
    return "reserved";
  }

  async finalize(record: MeteringRecord): Promise<void> {
    this.#activeReservations.delete(`${record.tenantId}\0${record.requestId}`);
    this.#records.push(Object.freeze({ ...record }));
  }

  recordsForTenant(tenantId: string): MeteringRecord[] {
    return this.#records
      .filter((record) => record.tenantId === tenantId)
      .map((record) => ({ ...record }));
  }
}
