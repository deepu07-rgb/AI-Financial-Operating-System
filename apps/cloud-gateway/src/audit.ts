export type GatewayAuditEventType =
  "analysis_completed" | "analysis_rejected" | "authentication_failed";

export interface GatewayAuditEvent {
  eventType: GatewayAuditEventType;
  tenantId: string | null;
  subject: string | null;
  requestId: string | null;
  outcomeCode: string;
  occurredAtMs: number;
}

export interface GatewayAuditSink {
  append(event: GatewayAuditEvent): Promise<void>;
}

export class InMemoryGatewayAuditSink implements GatewayAuditSink {
  readonly #events: GatewayAuditEvent[] = [];

  async append(event: GatewayAuditEvent): Promise<void> {
    this.#events.push(Object.freeze({ ...event }));
  }

  events(): GatewayAuditEvent[] {
    return this.#events.map((event) => ({ ...event }));
  }
}
