import { useId, useState } from "react";

import type {
  AvailableAction,
  CommandCode,
  Provider,
  RepositoryId,
  RepositoryReference,
  SessionObservabilityProjection,
  WorkItemId,
  WorkspaceId,
} from "../../../core/generated";
import { useResumeSessionMutation, useStartSessionMutation } from "../hooks/useSessionControlMutations";

interface SessionControlsProps {
  workspaceId: WorkspaceId;
  workItemId: WorkItemId;
  sessions: SessionObservabilityProjection[];
  repositories: RepositoryReference[];
  actions: AvailableAction[];
  revision: number;
}

const providers: Provider[] = ["claude", "codex"];

function actionFor(actions: AvailableAction[], code: CommandCode) {
  return actions.find((action) => action.code === code);
}

function liveRank(session: SessionObservabilityProjection) {
  if (session.bindingState === "current") return 0;
  if (session.liveness.state === "active") return 1;
  if (session.liveness.state === "idle") return 2;
  return 3;
}

export function orderSessions(sessions: SessionObservabilityProjection[]) {
  return [...sessions].sort((left, right) => {
    const rank = liveRank(left) - liveRank(right);
    if (rank !== 0) return rank;
    const activity = (right.lastActivityAt ?? "").localeCompare(left.lastActivityAt ?? "");
    if (activity !== 0) return activity;
    return left.id.localeCompare(right.id);
  });
}

export function isResumable(session: SessionObservabilityProjection) {
  return (
    (session.resumability === "validated" || session.resumability === "preflight_passed") &&
    session.liveness.state !== "active"
  );
}

export function SessionControls({ workspaceId, workItemId, sessions, repositories, actions, revision }: SessionControlsProps) {
  const repositoryId = useId();
  const providerId = useId();
  const [repository, setRepository] = useState<RepositoryId | "">(repositories.length === 1 ? repositories[0].id : "");
  const [provider, setProvider] = useState<Provider>("codex");
  const [selected, setSelected] = useState<string | undefined>(undefined);

  const start = useStartSessionMutation(workspaceId, workItemId);
  const resume = useResumeSessionMutation(workspaceId, workItemId);
  const busy = start.isPending || resume.isPending;

  const startAction = actionFor(actions, "start_session");
  const resumeAction = actionFor(actions, "resume_session");
  const focusAction = actionFor(actions, "focus_session");
  const followUpAction = actionFor(actions, "follow_up_session");
  const recoverAction = actionFor(actions, "recover_session");
  const expectedRevision = startAction?.expectedRevision ?? resumeAction?.expectedRevision ?? revision;

  const ordered = orderSessions(sessions);
  const resumableSessions = ordered.filter(isResumable);
  const chosen = selected ?? resumableSessions[0]?.id;
  const requiresRepositoryChoice = repositories.length > 1;
  const startBlocked = startAction?.available !== true || busy || (requiresRepositoryChoice && repository === "");

  const failure = start.data?.error ?? resume.data?.error;
  const transportError = start.error ?? resume.error;

  return (
    <section id="session-controls" tabIndex={-1} aria-labelledby="session-controls-title" className="scroll-mt-6 space-y-4 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5">
      <h2 id="session-controls-title" className="text-lg font-semibold">Session controls</h2>
      <p className="text-sm text-[var(--muted-text)]">
        {ordered.length === 0
          ? "No session is bound to this Work item."
          : `${ordered.length} bound ${ordered.length === 1 ? "session" : "sessions"}.`}
      </p>

      {busy && <p role="status">Workboard is launching the session. It will appear here when it binds.</p>}
      {failure != null && (
        <p role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">
          {failure.message}
          <span className="ml-2 text-xs text-[var(--muted-text)]">{failure.code}</span>
        </p>
      )}
      {transportError != null && <p role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">The session request could not reach Workboard. Retry when the daemon is reachable.</p>}

      <div className="space-y-3 rounded-lg border border-[var(--border)] p-4">
        <h3 className="font-semibold">{ordered.length === 0 ? "Start a session" : "Start another session"}</h3>
        {requiresRepositoryChoice && (
          <div>
            <label htmlFor={repositoryId} className="block text-sm font-semibold">Repository</label>
            <select
              id={repositoryId}
              value={repository}
              onChange={(event) => setRepository(event.target.value as RepositoryId | "")}
              disabled={startAction?.available !== true || busy}
              className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--canvas)] p-2 text-sm disabled:opacity-50"
            >
              <option value="">Select the launch repository</option>
              {repositories.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.title}</option>)}
            </select>
          </div>
        )}
        <div>
          <label htmlFor={providerId} className="block text-sm font-semibold">Provider</label>
          <select
            id={providerId}
            value={provider}
            onChange={(event) => setProvider(event.target.value as Provider)}
            disabled={startAction?.available !== true || busy}
            className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--canvas)] p-2 text-sm disabled:opacity-50"
          >
            {providers.map((candidate) => <option key={candidate} value={candidate}>{candidate}</option>)}
          </select>
        </div>
        <button
          type="button"
          disabled={startBlocked}
          onClick={() => start.mutate({ expectedRevision, repositoryId: repository === "" ? null : repository, provider })}
          className="rounded-lg border border-[var(--border)] px-3 py-2 disabled:opacity-50"
        >
          {ordered.length === 0 ? "Start session" : "Start another"}
        </button>
        {startAction?.available !== true && startAction?.unavailableReason != null && (
          <p className="text-sm text-[var(--muted-text)]">{startAction.unavailableReason.message}</p>
        )}
      </div>

      {ordered.length > 0 && (
        <div className="space-y-3 rounded-lg border border-[var(--border)] p-4">
          <h3 className="font-semibold">Resume a session</h3>
          <ul className="space-y-2">
            {ordered.map((session) => (
              <li key={session.id} className="rounded-lg border border-[var(--border)] p-3">
                <label className="flex items-start gap-3">
                  <input
                    type="radio"
                    name="workboard-resume-session"
                    value={session.id}
                    checked={chosen === session.id}
                    disabled={!isResumable(session) || busy}
                    onChange={() => setSelected(session.id)}
                    className="mt-1"
                  />
                  <span>
                    <strong>{session.provider}</strong>
                    <span className="ml-2">{session.role.replaceAll("_", " ")}</span>
                    <span className="block text-sm text-[var(--muted-text)]">
                      {session.liveness.state.replaceAll("_", " ")}
                      {session.liveness.stale ? " · stale evidence" : ""}
                      {" · "}
                      {session.resumability.replaceAll("_", " ")}
                      {" · "}
                      {session.primaryWriter.replaceAll("_", " ")}
                    </span>
                    {!isResumable(session) && (
                      <span className="block text-sm">
                        {session.liveness.state === "active"
                          ? "Already running. Workboard will not launch a duplicate."
                          : "No validated resume evidence for this session."}
                      </span>
                    )}
                  </span>
                </label>
              </li>
            ))}
          </ul>
          <button
            type="button"
            disabled={resumeAction?.available !== true || busy || chosen === undefined}
            onClick={() => chosen !== undefined && resume.mutate({ expectedRevision, sessionId: chosen })}
            className="rounded-lg border border-[var(--border)] px-3 py-2 disabled:opacity-50"
          >
            Resume
          </button>
          {resumeAction?.available !== true && resumeAction?.unavailableReason != null && (
            <p className="text-sm text-[var(--muted-text)]">{resumeAction.unavailableReason.message}</p>
          )}
        </div>
      )}

      <ul className="space-y-1 text-sm text-[var(--muted-text)]">
        {[focusAction, followUpAction, recoverAction].map((action) =>
          action?.unavailableReason == null ? null : (
            <li key={action.code}>
              <strong>{action.code.replaceAll("_", " ")}:</strong> {action.unavailableReason.message}
              <span className="ml-2 text-xs">{action.unavailableReason.code}</span>
            </li>
          ),
        )}
      </ul>
    </section>
  );
}
