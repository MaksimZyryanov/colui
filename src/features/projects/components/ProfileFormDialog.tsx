import { useState } from 'react';
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
export function ProfileFormDialog({ profile, details, onClose }: Props) {
  const [stage, setStage] = useState(0);
  const [draft, setDraft] = useState<ProfileDraft>(details ? { displayName: details.profile.displayName, composeProjectName: details.profile.composeProjectName, workingDirectory: details.profile.workingDirectory, composeFiles: details.composeFiles, environmentFiles: details.environmentFiles } : blank);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [backendIssues, setBackendIssues] = useState<string[]>([]);
  const mutations = useProfileMutations();
  const update = (field: keyof ProfileDraft, value: string | string[]) => setDraft(current => ({ ...current, [field]: value }));
  const saveOffline = () => { const immediate = validateProfileDraft(draft); if (immediate) { setErrors(immediate); return; } localStorage.setItem(`project-draft:${profile?.id ?? 'new'}`, JSON.stringify(draft)); onClose(); };
  const submit = async () => {
    const immediate = validateProfileDraft(draft); if (immediate) { setErrors(immediate); return; }
    const inspected = await inspectProfileDraft(draft); if (!inspected.valid) { setBackendIssues(inspected.issues.map(issue => issue.message)); return; }
    try { if (profile) await mutations.update.mutateAsync({ profileId: profile.id, expectedRevision: profile.revision, patch: draft }); else await mutations.create.mutateAsync(draft); onClose(); } catch (error) { if (error instanceof AppErrorException && error.code === 'profile_revision_conflict') setBackendIssues([error.message]); else setBackendIssues([error instanceof Error ? error.message : 'Unable to save profile']); }
  };
  return <Dialog open onOpenChange={open => { if (!open) onClose(); }} title={profile ? 'Edit Project' : 'Add Project'} description="Configure project profile"><form onSubmit={event => { event.preventDefault(); if (stage === 0) { setStage(1); return; } void submit(); }}>{stage === 0 ? <><label>Display name<Input value={draft.displayName} aria-invalid={Boolean(errors.displayName)} aria-describedby={errors.displayName ? 'displayName-error' : undefined} onChange={event => update('displayName', event.target.value)} /></label>{errors.displayName ? <span id="displayName-error">{errors.displayName}</span> : null}<label>Compose name<Input value={draft.composeProjectName} aria-invalid={Boolean(errors.composeProjectName)} aria-describedby={errors.composeProjectName ? 'composeProjectName-error' : undefined} onChange={event => update('composeProjectName', event.target.value)} /></label>{errors.composeProjectName ? <span id="composeProjectName-error">{errors.composeProjectName}</span> : null}<label>Working directory<Input value={draft.workingDirectory} aria-invalid={Boolean(errors.workingDirectory)} aria-describedby={errors.workingDirectory ? 'workingDirectory-error' : undefined} onChange={event => update('workingDirectory', event.target.value)} /></label>{errors.workingDirectory ? <span id="workingDirectory-error">{errors.workingDirectory}</span> : null}</> : <><label>Compose files<Input value={draft.composeFiles.join('\n')} aria-invalid={Boolean(errors.composeFiles)} aria-describedby={errors.composeFiles ? 'composeFiles-error' : undefined} onChange={event => update('composeFiles', event.target.value.split('\n').filter(Boolean))} /></label><label>Environment files<Input value={draft.environmentFiles.join('\n')} onChange={event => update('environmentFiles', event.target.value.split('\n').filter(Boolean))} /></label>{errors.composeFiles ? <span id="composeFiles-error">{errors.composeFiles}</span> : null}</>}{backendIssues.length ? <Alert variant="destructive"><ul>{backendIssues.map(issue => <li key={issue}>{issue}</li>)}</ul></Alert> : null}<div className="ui-dialog-actions">{stage === 1 ? <Button type="button" onClick={() => setStage(0)}>Back</Button> : null}<Button type="button" onClick={saveOffline}>Save offline</Button><Button type="submit">{stage === 0 ? 'Next' : 'Save Project'}</Button></div></form></Dialog>;
}
