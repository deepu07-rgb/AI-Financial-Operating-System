import { invoke } from "@tauri-apps/api/core";

export type PilotMetricStatus =
  "met" | "in_progress" | "attention" | "measured" | "not_measured";

export interface PilotMetricCard {
  id: string;
  label: string;
  value: number;
  target?: number;
  unit: "documents" | "milliseconds" | "basis_points" | "actions" | "backups";
  status: PilotMetricStatus;
  detail: string;
}

export interface OnboardingStep {
  id: string;
  label: string;
  complete: boolean;
  detail: string;
}

export interface PilotDashboardResponse {
  generatedAtMs: number;
  activationAchieved: boolean;
  activationDetail: string;
  metricCards: PilotMetricCard[];
  onboardingSteps: OnboardingStep[];
  measurableLocally: string[];
  requiresPilotEvidence: string[];
}

export interface SupportBundlePreview {
  token: string;
  sha256: string;
  byteSize: number;
  expiresAtMs: number;
  bundle: Record<string, unknown>;
}

export interface SupportBundleExport {
  cancelled: boolean;
  filename?: string;
  sha256?: string;
  byteSize?: number;
}

export function getPilotDashboard(
  workspaceId: string,
): Promise<PilotDashboardResponse> {
  return invoke("get_pilot_dashboard", { workspaceId });
}

export function prepareSupportBundle(
  workspaceId: string,
): Promise<SupportBundlePreview> {
  return invoke("prepare_support_bundle", { workspaceId });
}

export function exportSupportBundle(input: {
  workspaceId: string;
  actorUserId: string;
  token: string;
  expectedSha256: string;
}): Promise<SupportBundleExport> {
  return invoke("export_support_bundle", { input });
}
