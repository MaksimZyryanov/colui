import { profileDraftSchema, profileIdRequestSchema, removeProfileRequestSchema, updateProfileRequestSchema } from './schemas';
import { AppErrorException } from './errors';
import type { ProfileDraft, ProfileSummary } from './types';

let nextId = 1;
const id = () => `00000000-0000-0000-0000-${String(nextId++).padStart(12, '0')}`;
const now = () => new Date().toISOString();
const error = (code: AppErrorException['code'], operation: string, message: string, subjectId: string | null = null): never => { throw new AppErrorException({ code, operation, subjectId, message, details: null, retryable: false }); };
const validateDraft = (draft: ProfileDraft, operation: string) => { if (!draft.displayName || !/^[a-z0-9][a-z0-9_-]*$/.test(draft.composeProjectName) || draft.composeFiles.length === 0 || new Set([...draft.composeFiles, ...draft.environmentFiles]).size !== draft.composeFiles.length + draft.environmentFiles.length) error('profile_invalid', operation, 'Invalid profile draft'); };
let profiles: ProfileSummary[] = [];
let drafts = new Map<string, ProfileDraft>();
let runtime: unknown = { state: 'disconnected' };
let statuses = new Map<string, unknown>();
const overrides = new Map<string, unknown>();
let invocations: Array<{ command: string; args?: unknown }> = [];
const baseStatus = (profileId: string) => ({ profileId, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [] });
function validateId(args: unknown, operation: string) { const result = profileIdRequestSchema.safeParse(args); if (!result.success) error('profile_invalid', operation, 'profileId must be a UUID'); return result.data!.profileId; }
export const mockBackend = {
  reset() { profiles = []; drafts = new Map(); runtime = { state: 'disconnected' }; statuses = new Map(); overrides.clear(); invocations = []; nextId = 1; },
  getInvocations() { return [...invocations]; },
  setResponseOverride(command: string, value: unknown) { overrides.set(command, value); },
  async invoke(command: string, args?: unknown): Promise<unknown> {
    invocations.push({ command, args });
    if (overrides.has(command)) return overrides.get(command);
    switch (command) {
      case 'list_profiles': if (args !== undefined) error('profile_invalid', command, 'unexpected arguments'); return profiles;
      case 'get_profile': { const profileId = validateId(args, command); const profile = profiles.find(p => p.id === profileId); if (!profile) error('profile_not_found', command, 'Profile not found', profileId); const draft = drafts.get(profileId)!; return { profile, composeFiles: draft.composeFiles, environmentFiles: draft.environmentFiles }; }
      case 'inspect_profile_draft': { const result = profileDraftSchema.safeParse(args); if (!result.success) return { valid: false, issues: [{ message: 'Invalid profile draft' }] }; try { validateDraft(result.data, command); return { valid: true, issues: [] }; } catch (caught) { return { valid: false, issues: [{ message: (caught as AppErrorException).message }] }; } }
       case 'create_profile': { const parsed = profileDraftSchema.safeParse(args); if (!parsed.success) error('profile_invalid', command, 'Invalid profile draft'); const draft = parsed.data!; validateDraft(draft, command); const profileId = id(); const profile: ProfileSummary = { id: profileId, revision: 1, displayName: draft.displayName, composeProjectName: draft.composeProjectName, workingDirectory: draft.workingDirectory, registrationOrigin: 'manual' }; profiles = [...profiles, profile]; drafts.set(profileId, draft); statuses.set(profileId, baseStatus(profileId)); return profile; }
       case 'update_profile': { const parsed = updateProfileRequestSchema.safeParse(args); if (!parsed.success) error('profile_invalid', command, 'Invalid update request'); const { profileId, expectedRevision, patch } = parsed.data!; validateDraft(patch, command); const profile = profiles.find(p => p.id === profileId); if (!profile) error('profile_not_found', command, 'Profile not found', profileId); if (profile!.revision !== expectedRevision) error('profile_revision_conflict', command, 'Profile revision conflict', profileId); const updated: ProfileSummary = { ...profile!, revision: profile!.revision + 1, displayName: patch.displayName, composeProjectName: patch.composeProjectName, workingDirectory: patch.workingDirectory }; profiles = profiles.map(p => p.id === profileId ? updated : p); drafts.set(profileId, patch); return updated; }
      case 'remove_profile': { const parsed = removeProfileRequestSchema.safeParse(args); if (!parsed.success) error('profile_invalid', command, 'Invalid remove request'); const request = parsed.data!; const profile = profiles.find(p => p.id === request.profileId); if (!profile) error('profile_not_found', command, 'Profile not found', request.profileId); if (profile!.revision !== request.expectedRevision) error('profile_revision_conflict', command, 'Profile revision conflict', profile!.id); profiles = profiles.filter(p => p.id !== profile!.id); drafts.delete(profile!.id); statuses.delete(profile!.id); return undefined; }
      case 'get_runtime_state': return runtime;
      case 'connect_runtime': runtime = { state: 'ready', context: { sessionId: id(), endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: now() } }; return runtime;
      case 'get_project_status': { const profileId = validateId(args, command); if (!profiles.some(p => p.id === profileId)) error('profile_not_found', command, 'Profile not found', profileId); return statuses.get(profileId) ?? baseStatus(profileId); }
      case 'apply_project': case 'stop_project': case 'tear_down_project': case 'restart_project': { const profileId = validateId(args, command); if (!profiles.some(p => p.id === profileId)) error('profile_not_found', command, 'Profile not found', profileId); const activity = command === 'stop_project' || command === 'tear_down_project' ? 'none-running' : 'all-running'; const kind = command.replace('_project', '').replace('_', '-') as 'apply' | 'stop' | 'tear-down' | 'restart'; const status = { ...(statuses.get(profileId) as object ?? baseStatus(profileId)), runtime: { presence: 'present', activity, containerCount: 1, runningContainerCount: activity === 'none-running' ? 0 : 1, observedAt: now() }, operation: { kind, phase: 'succeeded', startedAt: now() } }; statuses.set(profileId, status); return { profileId, success: true }; }
      default: error('protocol_mismatch', command, `Unknown command: ${command}`);
    }
  },
};
