import { useCallback, useEffect, useMemo, useState } from "react";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import { getOperationsWorkspace, type OperationsWorkspace } from "./operations";

export function useOperationsWorkspace(workspaceId: string) {
  const [data, setData] = useState<OperationsWorkspace>();
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [workspace, localUsers] = await Promise.all([
        getOperationsWorkspace(workspaceId),
        listLocalUsers(workspaceId),
      ]);
      setData(workspace);
      setUsers(localUsers);
      setActorUserId((current) =>
        localUsers.some((user) => user.id === current)
          ? current
          : (localUsers.find((user) => user.role !== "reviewer")?.id ??
            localUsers[0]?.id ??
            ""),
      );
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

  useEffect(() => {
    let active = true;
    void Promise.all([
      getOperationsWorkspace(workspaceId),
      listLocalUsers(workspaceId),
    ])
      .then(([workspace, localUsers]) => {
        if (!active) return;
        setData(workspace);
        setUsers(localUsers);
        setActorUserId((current) =>
          localUsers.some((user) => user.id === current)
            ? current
            : (localUsers.find((user) => user.role !== "reviewer")?.id ??
              localUsers[0]?.id ??
              ""),
        );
      })
      .catch((caught: unknown) => {
        if (active)
          setError(caught instanceof Error ? caught.message : String(caught));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [workspaceId]);

  const actor = useMemo(
    () => users.find((user) => user.id === actorUserId),
    [actorUserId, users],
  );
  const canMutate = actor?.role !== "reviewer" && Boolean(actor);

  return {
    actor,
    actorUserId,
    canMutate,
    data,
    error,
    loading,
    refresh,
    setActorUserId,
    setData,
    setError,
    users,
  };
}
