import { useId, useState, type ReactNode } from "react";

import { Alert } from "../../../components/ui/alert";
import { Button } from "../../../components/ui/button";
import { Card, CardTitle } from "../../../components/ui/card";
import { Input } from "../../../components/ui/input";
import { Label } from "../../../components/ui/label";
import { Select } from "../../../components/ui/select";
import { Textarea } from "../../../components/ui/textarea";
import type { WorkItemDetail, WorkItemStateInput } from "../../../core/contracts";
import type { useCheckpointWorkItemMutation } from "../hooks/useCheckpointWorkItemMutation";
import { allowedWorkItemStatuses, createWorkItemStateSchema, type WorkItemStateForm } from "../schemas/workItemStateSchema";

interface WorkItemStateEditorProps {
  detail: WorkItemDetail;
  refresh(): void;
  checkpoint: ReturnType<typeof useCheckpointWorkItemMutation>;
}

const statuses = ["backlog", "ready", "in_progress", "blocked", "review", "done", "cancelled"] as const;
const nextActions = ["actionable", "blocked", "paused", "review", "delivery"] as const;
const reviewStatuses = ["not_started", "in_progress", "changes_requested", "ready", "accepted"] as const;
const deliveryStatuses = ["not_started", "in_progress", "blocked", "ready", "delivered"] as const;

export function WorkItemStateEditor({ detail, refresh, checkpoint }: WorkItemStateEditorProps) {
  const [form, setFormState] = useState<WorkItemStateForm>(() => initialForm(detail));
  const [validation, setValidation] = useState<Array<{ path: string; message: string }>>([]);
  const reconciliation = detail.structuredState.reconciliation;
  const remoteError = checkpoint.data?.error;
  const saved = checkpoint.data?.result?.type === "work_item_detail" && checkpoint.data.error == null;
  const schema = createWorkItemStateSchema(detail.status);
  const allowedStatuses = allowedWorkItemStatuses(detail.status);
  const setForm = (value: WorkItemStateForm) => {
    checkpoint.reset();
    setValidation([]);
    setFormState(value);
  };
  const refreshAuthoritative = () => {
    checkpoint.reset();
    setValidation([]);
    refresh();
  };

  const submit = () => {
    const parsed = schema.safeParse(form);
    if (!parsed.success) {
      setValidation(parsed.error.issues.map((issue) => ({
        path: issue.path.map(String).join(" → "),
        message: issue.message,
      })));
      return;
    }
    setValidation([]);
    checkpoint.mutate({ expectedRevision: detail.revision, state: parsed.data as WorkItemStateInput });
  };

  return (
    <Card asChild size="compact" className="p-5">
      <section id="state-editor" tabIndex={-1} aria-labelledby="state-editor-title" className="scroll-mt-6">
        <CardTitle id="state-editor-title">Update durable Work-item state</CardTitle>
        <p className="mt-2 text-sm text-muted-foreground">Saves the complete revision-checked state through Workboard and publishes it to the planning store.</p>

        {reconciliation != null && (
          <Alert role="alert" className="mt-4">
            <strong>Reconciliation required</strong>
            <p>{reconciliation.reason}</p>
            <Button type="button" className="mt-3" onClick={refreshAuthoritative}>Refresh authoritative state</Button>
          </Alert>
        )}

        <form className="mt-5 space-y-6" onSubmit={(event) => { event.preventDefault(); submit(); }}>
          <Field label="Current state" htmlFor="work-item-current-state">
            <Textarea id="work-item-current-state" rows={5} value={form.currentState} onChange={(event) => setForm({ ...form, currentState: event.target.value })} />
          </Field>

          <div className="grid gap-4 md:grid-cols-2">
            <Field label="Status" htmlFor="work-item-status">
              <Select id="work-item-status" value={form.status} onChange={(event) => setForm({ ...form, status: event.target.value as WorkItemStateForm["status"] })}>
                {statuses.map((status) => <option key={status} value={status} disabled={!allowedStatuses.includes(status)}>{status === "done" ? "done — set by integration" : label(status)}</option>)}
              </Select>
            </Field>
            <Field label="Terminal intent" htmlFor="work-item-terminal-intent">
              <Select id="work-item-terminal-intent" value={form.terminalIntent ?? ""} onChange={(event) => setForm({ ...form, terminalIntent: event.target.value === "" ? null : event.target.value as "complete" | "cancel" })}>
                <option value="">None</option>
                <option value="complete">Complete after integration</option>
                <option value="cancel">Cancel</option>
              </Select>
            </Field>
          </div>

          <div className="grid gap-4 md:grid-cols-[12rem_1fr]">
            <Field label="Next action kind" htmlFor="work-item-next-kind">
              <Select id="work-item-next-kind" value={form.nextAction.kind} onChange={(event) => setForm({ ...form, nextAction: { ...form.nextAction, kind: event.target.value as WorkItemStateForm["nextAction"]["kind"] } })}>
                {nextActions.map((kind) => <option key={kind} value={kind}>{label(kind)}</option>)}
              </Select>
            </Field>
            <Field label="Concrete next action" htmlFor="work-item-next-action">
              <Textarea id="work-item-next-action" rows={3} value={form.nextAction.description} onChange={(event) => setForm({ ...form, nextAction: { ...form.nextAction, description: event.target.value } })} />
            </Field>
          </div>

          <Collection title="Blockers" addLabel="Add blocker" onAdd={() => setForm({ ...form, blockers: [...form.blockers, { description: "", owner: "", unblockAction: "", resumeWhen: "" }] })}>
            {form.blockers.map((blocker, index) => (
              <EditorRow key={index} remove={() => setForm({ ...form, blockers: form.blockers.filter((_, candidate) => candidate !== index) })}>
                <TextInput label="Description" value={blocker.description} onChange={(description) => updateAt(form.blockers, index, { ...blocker, description }, (blockers) => setForm({ ...form, blockers }))} />
                <TextInput label="Owner" value={blocker.owner} onChange={(owner) => updateAt(form.blockers, index, { ...blocker, owner }, (blockers) => setForm({ ...form, blockers }))} />
                <TextInput label="Unblock action" value={blocker.unblockAction} onChange={(unblockAction) => updateAt(form.blockers, index, { ...blocker, unblockAction }, (blockers) => setForm({ ...form, blockers }))} />
                <TextInput label="Resume when" value={blocker.resumeWhen} onChange={(resumeWhen) => updateAt(form.blockers, index, { ...blocker, resumeWhen }, (blockers) => setForm({ ...form, blockers }))} />
              </EditorRow>
            ))}
          </Collection>

          <Collection title="Decisions" addLabel="Add decision" onAdd={() => setForm({ ...form, decisions: [...form.decisions, { decision: "", rationale: "" }] })}>
            {form.decisions.map((decision, index) => (
              <EditorRow key={index} remove={() => setForm({ ...form, decisions: form.decisions.filter((_, candidate) => candidate !== index) })}>
                <TextInput label="Decision" value={decision.decision} onChange={(value) => updateAt(form.decisions, index, { ...decision, decision: value }, (decisions) => setForm({ ...form, decisions }))} />
                <TextInput label="Rationale" value={decision.rationale} onChange={(rationale) => updateAt(form.decisions, index, { ...decision, rationale }, (decisions) => setForm({ ...form, decisions }))} />
              </EditorRow>
            ))}
          </Collection>

          <Collection title="Verification" addLabel="Add verification" onAdd={() => setForm({ ...form, verification: [...form.verification, { check: "", result: "not_run", evidence: null }] })}>
            {form.verification.map((verification, index) => (
              <EditorRow key={index} remove={() => setForm({ ...form, verification: form.verification.filter((_, candidate) => candidate !== index) })}>
                <TextInput label="Check" value={verification.check} onChange={(check) => updateAt(form.verification, index, { ...verification, check }, (items) => setForm({ ...form, verification: items }))} />
                <Field label="Result" htmlFor={`verification-result-${index}`}>
                  <Select id={`verification-result-${index}`} value={verification.result} onChange={(event) => updateAt(form.verification, index, { ...verification, result: event.target.value as typeof verification.result }, (items) => setForm({ ...form, verification: items }))}>
                    <option value="not_run">Not run</option><option value="passed">Passed</option><option value="failed">Failed</option>
                  </Select>
                </Field>
                <TextInput label="Evidence" value={verification.evidence ?? ""} onChange={(evidence) => updateAt(form.verification, index, { ...verification, evidence: evidence.trim() === "" ? null : evidence }, (items) => setForm({ ...form, verification: items }))} />
              </EditorRow>
            ))}
          </Collection>

          <div className="grid gap-5 md:grid-cols-2">
            <StatePhase title="Review" status={form.review.status} statuses={reviewStatuses} evidence={form.review.evidence} onStatus={(status) => setForm({ ...form, review: { ...form.review, status } })} onEvidence={(evidence) => setForm({ ...form, review: { ...form.review, evidence } })} />
            <StatePhase title="Delivery" status={form.delivery.status} statuses={deliveryStatuses} evidence={form.delivery.evidence} onStatus={(status) => setForm({ ...form, delivery: { ...form.delivery, status } })} onEvidence={(evidence) => setForm({ ...form, delivery: { ...form.delivery, evidence } })} />
          </div>

          {validation.length > 0 && <Alert id="state-validation-errors" role="alert"><strong>Review these state fields</strong><ul className="list-disc pl-5">{validation.map((issue, index) => <li key={`${issue.path}:${issue.message}:${index}`}><strong>{issue.path || "State"}:</strong> {issue.message}</li>)}</ul></Alert>}
          {remoteError != null && <Alert role="alert"><strong>{label(remoteError.code)}</strong><p>{remoteError.message}</p>{isStale(remoteError.code) && <Button type="button" className="mt-3" onClick={refreshAuthoritative}>Refresh and review changes</Button>}</Alert>}
          {checkpoint.isError && <Alert role="alert">Workboard is disconnected. The state was not reported as saved.</Alert>}
          <div aria-live="polite">{checkpoint.isPending ? "Saving authoritative state..." : saved ? "Authoritative Work-item state saved." : ""}</div>
          <Button type="submit" variant="solid" disabled={checkpoint.isPending || reconciliation != null}>{checkpoint.isPending ? "Saving..." : "Save durable state"}</Button>
        </form>
      </section>
    </Card>
  );
}

function initialForm(detail: WorkItemDetail): WorkItemStateForm {
  const state = detail.structuredState.state;
  return state == null ? {
    schemaVersion: 1,
    expectedStateRevision: 0,
    expectedDocumentRevision: detail.structuredState.documentRevision,
    currentState: "",
    nextAction: { kind: "actionable", description: "" },
    blockers: [], decisions: [], verification: [],
    review: { status: "not_started", evidence: [] },
    delivery: { status: "not_started", evidence: [] },
    status: detail.status,
    terminalIntent: null,
  } : {
    schemaVersion: 1,
    expectedStateRevision: state.revision,
    expectedDocumentRevision: state.documentRevision,
    currentState: state.currentState,
    nextAction: state.nextAction,
    blockers: state.blockers,
    decisions: state.decisions,
    verification: state.verification,
    review: state.review,
    delivery: state.delivery,
    status: state.status,
    terminalIntent: state.terminalIntent,
  };
}

function Field({ label: fieldLabel, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  return <div className="grid gap-1.5"><Label htmlFor={htmlFor}>{fieldLabel}</Label>{children}</div>;
}

function TextInput({ label: fieldLabel, value, onChange }: { label: string; value: string; onChange(value: string): void }) {
  const id = useId();
  return <Field label={fieldLabel} htmlFor={id}><Input id={id} value={value} onChange={(event) => onChange(event.target.value)} /></Field>;
}

function Collection({ title, addLabel, onAdd, children }: { title: string; addLabel: string; onAdd(): void; children: ReactNode }) {
  return <fieldset className="space-y-3"><legend className="font-semibold">{title}</legend>{children}<Button type="button" size="sm" onClick={onAdd}>{addLabel}</Button></fieldset>;
}

function EditorRow({ remove, children }: { remove(): void; children: ReactNode }) {
  return <div className="grid gap-3 rounded-lg border border-border p-3 md:grid-cols-2">{children}<Button type="button" size="sm" onClick={remove}>Remove</Button></div>;
}

function StatePhase<T extends string>({ title, status, statuses: options, evidence, onStatus, onEvidence }: { title: string; status: T; statuses: readonly T[]; evidence: string[]; onStatus(value: T): void; onEvidence(value: string[]): void }) {
  const id = title.toLowerCase();
  return <fieldset className="space-y-3"><legend className="font-semibold">{title}</legend><Field label={`${title} status`} htmlFor={`${id}-status`}><Select id={`${id}-status`} value={status} onChange={(event) => onStatus(event.target.value as T)}>{options.map((option) => <option key={option} value={option}>{label(option)}</option>)}</Select></Field>{evidence.map((item, index) => <EditorRow key={index} remove={() => onEvidence(evidence.filter((_, candidate) => candidate !== index))}><TextInput label={`${title} evidence`} value={item} onChange={(value) => updateAt(evidence, index, value, onEvidence)} /></EditorRow>)}<Button type="button" size="sm" onClick={() => onEvidence([...evidence, ""])}>Add {title.toLowerCase()} evidence</Button></fieldset>;
}

function updateAt<T>(values: T[], index: number, value: T, update: (values: T[]) => void) {
  update(values.map((candidate, candidateIndex) => candidateIndex === index ? value : candidate));
}

function label(value: string) {
  return value.replaceAll("_", " ");
}

function isStale(code: string) {
  return code === "stale_revision" || code === "work_item_state_revision_stale" || code === "work_item_document_revision_stale" || code === "planning_document_concurrent_edit";
}
