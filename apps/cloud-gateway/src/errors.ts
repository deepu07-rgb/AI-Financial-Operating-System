export type GatewayErrorCode =
  | "authentication_failed"
  | "authorization_failed"
  | "budget_exceeded"
  | "duplicate_request"
  | "invalid_provider_output"
  | "invalid_request"
  | "payload_too_large"
  | "policy_denied"
  | "provider_unavailable"
  | "secure_transport_required";

const SAFE_MESSAGES: Record<GatewayErrorCode, string> = {
  authentication_failed: "Authentication failed.",
  authorization_failed: "The session is not authorized for cloud analysis.",
  budget_exceeded: "The tenant cloud-analysis budget has been reached.",
  duplicate_request: "This request identifier has already been used.",
  invalid_provider_output:
    "The provider did not return a valid structured response.",
  invalid_request: "The cloud-analysis request is invalid.",
  payload_too_large: "The request payload exceeds the permitted size.",
  policy_denied: "Tenant policy does not permit this cloud analysis.",
  provider_unavailable: "No approved cloud provider is configured.",
  secure_transport_required: "Secure transport is required.",
};

const HTTP_STATUS: Record<GatewayErrorCode, number> = {
  authentication_failed: 401,
  authorization_failed: 403,
  budget_exceeded: 429,
  duplicate_request: 409,
  invalid_provider_output: 502,
  invalid_request: 400,
  payload_too_large: 413,
  policy_denied: 403,
  provider_unavailable: 503,
  secure_transport_required: 400,
};

export class GatewayError extends Error {
  readonly code: GatewayErrorCode;
  readonly status: number;

  constructor(code: GatewayErrorCode) {
    super(SAFE_MESSAGES[code]);
    this.name = "GatewayError";
    this.code = code;
    this.status = HTTP_STATUS[code];
  }
}
