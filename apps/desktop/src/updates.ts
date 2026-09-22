import { invoke } from "@tauri-apps/api/core";

export interface UpdateStatus {
  currentVersion: string;
  configured: boolean;
  automaticChecks: boolean;
  offlineInstallerAvailable: boolean;
  reason: string | null;
}

export interface UpdateCheckResult {
  available: boolean;
  currentVersion: string;
  version: string | null;
  notes: string | null;
  publishedAt: string | null;
  byteSize: number | null;
}

export function getUpdateStatus(): Promise<UpdateStatus> {
  return invoke("get_update_status");
}

export function checkForUpdate(): Promise<UpdateCheckResult> {
  return invoke("check_for_update");
}

export function installUpdate(expectedVersion: string): Promise<void> {
  return invoke("install_update", { expectedVersion });
}
