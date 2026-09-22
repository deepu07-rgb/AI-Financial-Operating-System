import { createPublicKey, verify } from "node:crypto";
import type { KeyObject } from "node:crypto";
import { GatewayError } from "./errors.js";

export interface TenantClaims {
  issuer: string;
  audience: string;
  subject: string;
  tenantId: string;
  roles: string[];
  issuedAt: number;
  expiresAt: number;
  tokenId: string;
}

interface AuthenticatorOptions {
  publicKeyPem: string;
  issuer: string;
  audience: string;
  nowSeconds?: () => number;
}

const TOKEN_PATTERN = /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/u;
const IDENTIFIER_PATTERN = /^[a-zA-Z0-9][a-zA-Z0-9._:-]{0,127}$/u;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function decodeSegment(segment: string): unknown {
  try {
    const decoded = decodeCanonicalBase64Url(segment);
    return JSON.parse(decoded.toString("utf8"));
  } catch {
    throw new GatewayError("authentication_failed");
  }
}

function decodeCanonicalBase64Url(segment: string): Buffer {
  const decoded = Buffer.from(segment, "base64url");
  if (decoded.toString("base64url") !== segment) {
    throw new GatewayError("authentication_failed");
  }
  return decoded;
}

function parseClaims(value: unknown): TenantClaims {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "aud",
      "exp",
      "iat",
      "iss",
      "jti",
      "roles",
      "sub",
      "tenantId",
    ])
  ) {
    throw new GatewayError("authentication_failed");
  }
  const roles = value.roles;
  if (
    typeof value.iss !== "string" ||
    typeof value.aud !== "string" ||
    typeof value.sub !== "string" ||
    typeof value.tenantId !== "string" ||
    typeof value.jti !== "string" ||
    !IDENTIFIER_PATTERN.test(value.sub) ||
    !IDENTIFIER_PATTERN.test(value.tenantId) ||
    !IDENTIFIER_PATTERN.test(value.jti) ||
    !Array.isArray(roles) ||
    roles.length === 0 ||
    roles.length > 16 ||
    !roles.every(
      (role) => typeof role === "string" && IDENTIFIER_PATTERN.test(role),
    ) ||
    !Number.isSafeInteger(value.iat) ||
    !Number.isSafeInteger(value.exp)
  ) {
    throw new GatewayError("authentication_failed");
  }
  return {
    issuer: value.iss,
    audience: value.aud,
    subject: value.sub,
    tenantId: value.tenantId,
    roles: roles as string[],
    issuedAt: Number(value.iat),
    expiresAt: Number(value.exp),
    tokenId: value.jti,
  };
}

export class TenantAuthenticator {
  readonly #key: KeyObject;
  readonly #issuer: string;
  readonly #audience: string;
  readonly #nowSeconds: () => number;

  constructor(options: AuthenticatorOptions) {
    this.#key = createPublicKey(options.publicKeyPem);
    this.#issuer = options.issuer;
    this.#audience = options.audience;
    this.#nowSeconds =
      options.nowSeconds ?? (() => Math.floor(Date.now() / 1000));
  }

  authenticate(authorizationHeader: string | undefined): TenantClaims {
    if (!authorizationHeader?.startsWith("Bearer ")) {
      throw new GatewayError("authentication_failed");
    }
    const token = authorizationHeader.slice(7);
    if (token.length > 8192 || !TOKEN_PATTERN.test(token)) {
      throw new GatewayError("authentication_failed");
    }
    const [encodedHeader, encodedPayload, encodedSignature] = token.split(".");
    if (!encodedHeader || !encodedPayload || !encodedSignature) {
      throw new GatewayError("authentication_failed");
    }
    const header = decodeSegment(encodedHeader);
    if (
      !isRecord(header) ||
      !hasExactKeys(header, ["alg", "typ", "v"]) ||
      header.alg !== "EdDSA" ||
      header.typ !== "AIFOS-TENANT" ||
      header.v !== 1
    ) {
      throw new GatewayError("authentication_failed");
    }
    const signed = Buffer.from(`${encodedHeader}.${encodedPayload}`, "ascii");
    const signature = decodeCanonicalBase64Url(encodedSignature);
    if (!verify(null, signed, this.#key, signature)) {
      throw new GatewayError("authentication_failed");
    }
    const claims = parseClaims(decodeSegment(encodedPayload));
    const now = this.#nowSeconds();
    if (
      claims.issuer !== this.#issuer ||
      claims.audience !== this.#audience ||
      claims.issuedAt > now + 30 ||
      claims.expiresAt <= now ||
      claims.expiresAt - claims.issuedAt > 3600
    ) {
      throw new GatewayError("authentication_failed");
    }
    return claims;
  }
}
