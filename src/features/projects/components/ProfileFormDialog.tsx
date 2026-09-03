import { useEffect, useState } from 'react';
import type { ProfileDetails, ProfileDraft, ProfileSummary } from '../../../ipc/types';
import { inspectProfileDraft } from '../../../ipc/commands';
import { AppErrorException } from '../../../ipc/errors';
import { Button } from '../../../ui/components/Button';
import { Dialog } from '../../../ui/components/Dialog';
import { Input } from '../../../ui/components/Input';
import { Alert } from '../../../ui/components/Alert';
import { useProfileMutations } from '../hooks/useProfileMutations';

export function validateProfileDraft(draft: ProfileDraft): Record<keyof ProfileDraft, string> | null {
  const errors = {} as Record<keyof ProfileDraft, string>;
  if (!draft.displayName.trim()) errors.displayName = 'Display name is required';
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(draft.composeProjectName)) errors.composeProjectName = 'Use lowercase letters, numbers, hyphens, or underscores';
  if (!draft.workingDirectory.trim()) errors.workingDirectory = 'Working directory is required';
  if (!draft.composeFiles.length) errors.composeFiles = 'At least one Compose file is required';
  if (new Set([...draft.composeFiles, ...draft.environmentFiles]).size !== draft.composeFiles.length + draft.environmentFiles.length) errors.composeFiles = 'File paths must be unique';
  return Object.keys(errors).length ? errors : null;
}

const blank: ProfileDraft = { displayName: '', composeProjectName: '', workingDirectory: '', composeFiles: ['compose.yml'], environmentFiles: [] };
type Props = { profile?: ProfileSummary; details?: ProfileDetails; onClose: () => void };
type FormIssue = { field?: string | null; message: string };
const draftFromDetails = (details: ProfileDetails): ProfileDraft => ({ displayName: details.profile.displayName, composeProjectName: details.profile.composeProjectName, workingDirectory: details.profile.workingDirectory, composeFiles: details.composeFiles, environmentFiles: details.environmentFiles });

export function ProfileFormDialog({ profile, details, onClose }: Props) {
  const [stage, setStage] = useState(0);
  const [draft, setDraft] = useState<ProfileDraft>(details ? draftFromDetails(details) : blank);
  const [dirtyFields, setDirtyFields] = useState<Set<keyof ProfileDraft>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [backendIssues, setBackendIssues] = useState<FormIssue[]>([]);
  const [inspectError, setInspectError] = useState<Error | AppErrorException | null>(null);
  const mutations = useProfileMutations();

  useEffect(() => {
    if (!details) return;
    const freshDraft = draftFromDetails(details);
    setDraft(current => ({ ...freshDraft, ...Object.fromEntries([...dirtyFields].map(field => [field, current[field]])) }));
  }, [details]);

  const update = (field: keyof ProfileDraft, value: string | string[]) => {
    setDirtyFields(current => new Set(current).add(field));
    setDraft(current => ({ ...current, [field]: value }));
  };
  const saveOffline = () => { const immediate = validateProfileDraft(draft); if (immediate) { setErrors(immediate); return; } window.localStorage.setItem(`project-draft:${profile?.id ?? 'new'}`, JSON.stringify(draft)); onClose(); };
  const submit = async () => {
    const immediate = validateProfileDraft(draft); if (immediate) { setErrors(immediate); return; }
    setInspectError(null); setBackendIssues([]);
    let inspected;
    try { inspected = await inspectProfileDraft(draft); } catch (error) { setInspectError(error instanceof Error ? error : new Error('Unable to inspect profile draft')); return; }
    if (!inspected.valid) { setBackendIssues(inspected.issues); return; }
    try { if (profile) await mutations.update.mutateAsync({ profileId: profile.id, expectedRevision: profile.revision, patch: draft }); else await mutations.create.mutateAsync(draft); onClose(); } catch (error) { setBackendIssues([{ message: error instanceof Error ? error.message : 'Unable to save profile' }]); }
  };
  const issueFor = (field: keyof ProfileDraft) => errors[field] ?? backendIssues.find(issue => issue.field === field)?.message;
  const fieldProps = (field: keyof ProfileDraft) => ({ 'aria-invalid': Boolean(issueFor(field)), 'aria-describedby': issueFor(field) ? `${field}-error` : undefined });
  return <Dialog open onOpenChange={open => { if (!open) onClose(); }} title={profile ? 'Edit Project' : 'Add Project'} description="Configure project profile"><form onSubmit={event => { event.preventDefault(); if (stage === 0) setStage(1); else void submit(); }}>{stage === 0 ? <><label>Display name<Input value={draft.displayName} {...fieldProps('displayName')} onChange={event => update('displayName', event.target.value)} /></label>{issueFor('displayName') ? <span id="displayName-error">{issueFor('displayName')}</span> : null}<label>Compose name<Input value={draft.composeProjectName} {...fieldProps('composeProjectName')} onChange={event => update('composeProjectName', event.target.value)} /></label>{issueFor('composeProjectName') ? <span id="composeProjectName-error">{issueFor('composeProjectName')}</span> : null}<label>Working directory<Input value={draft.workingDirectory} {...fieldProps('workingDirectory')} onChange={event => update('workingDirectory', event.target.value)} /></label>{issueFor('workingDirectory') ? <span id="workingDirectory-error">{issueFor('workingDirectory')}</span> : null}</> : <><label>Compose files<Input value={draft.composeFiles.join('\n')} {...fieldProps('composeFiles')} onChange={event => update('composeFiles', event.target.value.split('\n').filter(Boolean))} /></label>{issueFor('composeFiles') ? <span id="composeFiles-error">{issueFor('composeFiles')}</span> : null}<label>Environment files<Input value={draft.environmentFiles.join('\n')} {...fieldProps('environmentFiles')} onChange={event => update('environmentFiles', event.target.value.split('\n').filter(Boolean))} /></label>{issueFor('environmentFiles') ? <span id="environmentFiles-error">{issueFor('environmentFiles')}</span> : null}</>}{inspectError ? <Alert variant="destructive">{inspectError.message}</Alert> : null}{backendIssues.length ? <Alert variant="destructive" aria-label="Profile form errors"><ul aria-label="Profile form errors">{backendIssues.map((issue, index) => <li key={`${issue.field ?? 'form'}-${index}`}>{issue.message}</li>)}</ul></Alert> : null}<div className="ui-dialog-actions"><Button type="button" onClick={saveOffline}>Save offline</Button><Button type="submit">{stage === 0 ? 'Next' : 'Save'}</Button>{stage === 1 ? <Button type="button" onClick={() => setStage(0)}>Back</Button> : null}</div></form></Dialog>;
}
