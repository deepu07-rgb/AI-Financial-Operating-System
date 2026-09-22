import { invoke } from "@tauri-apps/api/core";
import type { LocalUserSummary } from "./dashboard";
import type { WorkspaceSummary } from "./review";

export interface CreateWorkspaceInput {
  name: string;
  baseCurrency: string;
  locale: string;
  financialYearStartMonth: number;
  dateFormat: "DD/MM/YYYY" | "MM/DD/YYYY" | "YYYY-MM-DD";
  taxLabel?: string;
}

export function createLocalWorkspace(
  input: CreateWorkspaceInput,
): Promise<WorkspaceSummary> {
  return invoke("create_local_workspace", { input });
}

export function createFirstOwner(
  workspaceId: string,
  displayName: string,
): Promise<LocalUserSummary> {
  return invoke("create_local_user", {
    workspaceId,
    input: { displayName, role: "owner" },
  });
}

export function createSampleWorkspace(): Promise<WorkspaceSummary> {
  return invoke("create_sample_workspace");
}

export function deleteSampleWorkspace(workspaceId: string): Promise<void> {
  return invoke("delete_sample_workspace", { workspaceId });
}
