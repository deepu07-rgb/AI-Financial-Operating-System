import { invoke } from "@tauri-apps/api/core";

export interface BackupRecord {
  workspaceId: string;
  workspaceName: string;
  archiveFilename: string;
  destination: string;
  archiveSha256: string;
  archiveByteSize: number;
  occurredAtMs: number;
}

export interface VerificationRecord {
  workspaceId: string;
  archiveFilename: string;
  archiveSha256: string;
  archiveByteSize: number;
  verifiedAtMs: number;
}

export interface RestoreRecord {
  workspaceId: string;
  workspaceName: string;
  archiveFilename: string;
  restoredAtMs: number;
}

export interface BackupStatus {
  reminderIntervalDays: number;
  lastSuccess: BackupRecord | null;
  lastVerification: VerificationRecord | null;
  lastRestore: RestoreRecord | null;
  reminderDue: boolean;
  nextReminderAtMs: number | null;
}

export interface BackupOperationResponse {
  cancelled: boolean;
  workspaceId?: string;
  workspaceName?: string;
  filename?: string;
  destination?: string;
  archiveSha256?: string;
  archiveByteSize?: number;
  fileCount?: number;
  tableCount?: number;
  occurredAtMs?: number;
  warnings?: string[];
}

export function getBackupStatus(): Promise<BackupStatus> {
  return invoke("get_backup_status");
}

export function configureBackupReminder(
  reminderIntervalDays: number,
): Promise<BackupStatus> {
  return invoke("configure_backup_reminder", {
    input: { reminderIntervalDays },
  });
}

export function createWorkspaceBackup(input: {
  workspaceId: string;
  actorUserId: string;
  passphrase: string;
}): Promise<BackupOperationResponse> {
  return invoke("create_workspace_backup", { input });
}

export function verifyWorkspaceBackup(input: {
  workspaceId: string;
  actorUserId: string;
  passphrase: string;
}): Promise<BackupOperationResponse> {
  return invoke("verify_workspace_backup", { input });
}

export function restoreWorkspaceBackup(
  passphrase: string,
): Promise<BackupOperationResponse> {
  return invoke("restore_workspace_backup", { input: { passphrase } });
}

export function exportWorkspacePortableData(input: {
  workspaceId: string;
  actorUserId: string;
}): Promise<BackupOperationResponse> {
  return invoke("export_workspace_portable_data", { input });
}
