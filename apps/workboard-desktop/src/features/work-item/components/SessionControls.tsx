import { useId, useState } from "react";

import { Alert } from "../../../components/ui/alert";
import { Button } from "../../../components/ui/button";
import { Card, CardTitle } from "../../../components/ui/card";
import { Label } from "../../../components/ui/label";
import { Radio } from "../../../components/ui/radio";
import { Select } from "../../../components/ui/select";
import type {
  AvailableAction,
  CommandCode,
  Provider,
  RepositoryId,
  RepositoryReference,
  Session,
  WorkItemId,
  WorkspaceId,
} from "../../../core/contracts";
import { useResumeSessionMutation, useStartSessionMutation } from "../hooks/useSessionControlMutations";

interface SessionControlsProps {
  workspaceId: WorkspaceId;
  workItemId: WorkItemId;
  sessions: Session[];
  repositories: RepositoryReference[];
  actions: AvailableAction[];
  revision: number;
}

const providers: Provider[] = ["claude", "codex"];

function actionFor(actions: AvailableAction[], code: CommandCode) {
  return actions.find((action) => action.code === code);
}

function liveRank(session: Session) {
  if (session.bindingState === "current") return 0;
  if (session.liveness.state === "active") return 1;
  if (session.liveness.state === "idle") return 2;
  return 3;
}

export function orderSessions(sessions: Session[]) {
  return [...sessions].sort((left, right) => {
    const rank = liveRank(left) - liveRank(right);
    if (rank !== 0) return rank;
    const activity = (right.lastActivityAt ?? "").localeCompare(left.lastActivityAt ?? "");
    if (activity !== 0) return activity;
    return left.id.localeCompare(right.id);
  });
}

export function isResumable(session: Session) {
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
    <Card asChild size="compact" className="p-5">
      <section id="session-controls" tabIndex={-1} aria-labelledby="session-controls-title" className="scroll-mt-6 space-y-4">
        <CardTitle id="session-controls-title">Session controls</CardTitle>
        <p className="text-sm text-muted-foreground">
          {ordered.length === 0
            ? "No session is bound to this Work item."
            : `${ordered.length} bound ${ordered.length === 1 ? "session" : "sessions"}.`}
        </p>

        {busy && <p role="status">Workboard is launching the session. It will appear here when it binds.</p>}
        {failure != null && (
          <Alert>
            {failure.message}
            <span className="ml-2 text-xs text-muted-foreground">{failure.code}</span>
          </Alert>
        )}
        {transportError != null && <Alert>The session request could not reach Workboard. Retry when the daemon is reachable.</Alert>}

        <div className="space-y-3 rounded-lg border border-border p-4">
          <h3 className="font-semibold">{ordered.length === 0 ? "Start a session" : "Start another session"}</h3>
          {requiresRepositoryChoice && (
            <div>
              <Label htmlFor={repositoryId} className="block font-semibold">Repository</Label>
              <Select
                id={repositoryId}
                value={repository}
                onChange={(event) => setRepository(event.target.value as RepositoryId | "")}
                disabled={startAction?.available !== true || busy}
                className="mt-1 w-full"
              >
                <option value="">Select the launch repository</option>
                {repositories.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.title}</option>)}
              </Select>
            </div>
          )}
          <div>
            <Label htmlFor={providerId} className="block font-semibold">Provider</Label>
            <Select
              id={providerId}
              value={provider}
              onChange={(event) => setProvider(event.target.value as Provider)}
              disabled={startAction?.available !== true || busy}
              className="mt-1 w-full"
            >
              {providers.map((candidate) => <option key={candidate} value={candidate}>{candidate}</option>)}
            </Select>
          </div>
          <Button
            type="button"
            disabled={startBlocked}
            onClick={() => start.mutate({ expectedRevision, repositoryId: repository === "" ? null : repository, provider })}
          >
            {ordered.length === 0 ? "Start session" : "Start another"}
          </Button>
          {startAction?.available !== true && startAction?.unavailableReason != null && (
            <p className="text-sm text-muted-foreground">{startAction.unavailableReason.message}</p>
          )}
        </div>

        {ordered.length > 0 && (
          <div className="space-y-3 rounded-lg border border-border p-4">
            <h3 className="font-semibold">Resume a session</h3>
            <ul className="space-y-2">
              {ordered.map((session) => (
                <li key={session.id} className="rounded-lg border border-border p-3">
                  <label className="flex items-start gap-3">
                    <Radio
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
                      <span className="block text-sm text-muted-foreground">
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
            <Button
              type="button"
              disabled={resumeAction?.available !== true || busy || chosen === undefined}
              onClick={() => chosen !== undefined && resume.mutate({ expectedRevision, sessionId: chosen })}
            >
              Resume
            </Button>
            {resumeAction?.available !== true && resumeAction?.unavailableReason != null && (
              <p className="text-sm text-muted-foreground">{resumeAction.unavailableReason.message}</p>
            )}
          </div>
        )}

        <ul className="space-y-1 text-sm text-muted-foreground">
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
    </Card>
  );
}
