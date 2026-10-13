import { profileDraftSchema, profileIdRequestSchema, removeProfileRequestSchema, updateProfileRequestSchema } from './schemas';
import { z } from 'zod';
import * as s from './schemas';
import { AppErrorException } from './errors';
import type { ProfileDraft, ProfileSummary, RuntimeState } from './types';

declare global { interface Window { __COLUI_MOCK_INVOCATIONS?: Array<{ command: string; args?: unknown }>; __COLUI_MOCK_PROFILE_ID?: string } }

const generatedIdPattern = /^00000000-0000-0000-0000-(\d{12})$/;
const nextPersistedId = (profiles: ProfileSummary[]) => profiles.reduce((next, profile) => {
  const match = generatedIdPattern.exec(profile.id);
  return match ? Math.max(next, Number(match[1]) + 1) : next;
}, 1);
const id = () => `00000000-0000-0000-0000-${String(nextId++).padStart(12, '0')}`;
const now = () => new Date().toISOString();
const error = (code: AppErrorException['code'], operation: string, message: string, subjectId: string | null = null): never => { throw new AppErrorException({ code, operation, subject: subjectId === null ? null : { kind: 'profile', id: subjectId }, message, details: null, retryable: false }); };
const validateDraft = (draft: ProfileDraft, operation: string) => { if (!draft.displayName || !/^[a-z0-9][a-z0-9_-]*$/.test(draft.composeProjectName) || draft.composeFiles.length === 0 || new Set([...draft.composeFiles, ...draft.environmentFiles]).size !== draft.composeFiles.length + draft.environmentFiles.length) error('profile_invalid', operation, 'Invalid profile draft'); };
const storageKey = 'colui.mock.profiles';
const storage = () => typeof process !== 'undefined' && process.release?.name === 'node'
  ? Object.getOwnPropertyDescriptor(globalThis, 'localStorage')?.value as Storage | undefined
  : window.localStorage;
const stored = () => { try { return JSON.parse(storage()?.getItem(storageKey) ?? 'null') as { profiles: ProfileSummary[]; drafts: Array<[string, ProfileDraft]>; statuses: Array<[string, unknown]> } | null; } catch { return null; } };
const initial = stored();
let profiles: ProfileSummary[] = initial?.profiles ?? [];
let nextId = nextPersistedId(profiles);
let drafts = new Map<string, ProfileDraft>(initial?.drafts ?? []);
let runtime: RuntimeState = { state: 'disconnected' };
let autoRegistrationEnabled = false;
let backup: ProfileSummary[] | null = null;
let backupDrafts = new Map<string, ProfileDraft>();
let registryRevision = 0;
const registryIdentity = () => ({ registryRevision, canonicalContentSha256: registryRevision.toString(16).padStart(64, '0') });
let inventoryGeneration = 0;
let inventoryObservedAt: string | null = null;
let inventoryContainerState: 'running' | 'stopped' = 'running';
const inventory = () => {
  if (inventoryGeneration === 0) return { generation: 0, hasSnapshot: false, observedAt: null, runtimeSessionId: null, daemonFingerprint: null, freshness: 'unavailable', lastSuccessfulObservedAt: null, containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null };
  const containers = profiles.map((profile, index) => ({ id: `mock-container-${index + 1}`, name: `${profile.composeProjectName}-web-1`, image: 'mock:latest', state: inventoryContainerState, statusText: inventoryContainerState === 'running' ? 'Up' : 'Exited', serviceName: 'web', publishedPorts: [] }));
  const projects = profiles.map((profile, index) => ({ composeProjectName: profile.composeProjectName, workingDirectory: profile.workingDirectory, configFiles: drafts.get(profile.id)?.composeFiles ?? [], containers: [containers[index]] }));
  return { generation: inventoryGeneration, hasSnapshot: true, observedAt: inventoryObservedAt, runtimeSessionId: runtime.state === 'ready' ? runtime.context.sessionId : '00000000-0000-0000-0000-000000000099', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: inventoryObservedAt, containers, projects, composeObservationGroups: projects.map(project => ({ composeProjectName: project.composeProjectName, workingDirectory: project.workingDirectory, configFiles: project.configFiles, containerIds: project.containers.map(container => container.id) })), standaloneContainers: [], error: null };
};
const publishInventory = (state: 'running' | 'stopped' = inventoryContainerState) => { inventoryGeneration += 1; inventoryObservedAt = now(); inventoryContainerState = state; return inventory(); };
let statuses = new Map<string, unknown>(initial?.statuses ?? []);
const overrides = new Map<string, unknown>();
const errors = new Map<string, unknown>();
let invocations: Array<{ command: string; args?: unknown }> = [];
const publishInvocations = () => { if (typeof window !== 'undefined') window.__COLUI_MOCK_INVOCATIONS = [...invocations]; };
const persist = () => storage()?.setItem(storageKey, JSON.stringify({ profiles, drafts: [...drafts], statuses: [...statuses] }));
const baseStatus = (profileId: string) => typeof window !== 'undefined' && /runtimeFailure|lifecycle/.test(window.location.search) ? { profileId, runtime: { presence: 'present', activity: 'all-running', containerCount: 1, runningContainerCount: 1, observedAt: now() }, definition: { state: 'valid', revision: '1', serviceCount: 1 }, operation: null, issues: [] } : { profileId, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [] };
function normalizeAbsolutePath(path: string): string | null {
  if (!path.startsWith('/')) return null;
  const components: string[] = [];
  for (const component of path.split('/')) {
    if (component === '..') components.pop();
    else if (component !== '' && component !== '.') components.push(component);
  }
  return `/${components.join('/')}`;
}
function profileAssociationKey(profile: ProfileSummary): string | null {
  const workingDirectory = normalizeAbsolutePath(profile.workingDirectory);
  const draft = drafts.get(profile.id);
  if (workingDirectory === null || !draft) return null;
  const configFiles = [...new Set(draft.composeFiles.map(file => normalizeAbsolutePath(file.startsWith('/') ? file : `${workingDirectory}/${file}`)))];
  return JSON.stringify([profile.composeProjectName, workingDirectory, configFiles]);
}
function requestArguments<T>(schema: z.ZodType<T, z.ZodTypeDef, unknown>, args: unknown, operation: string): T {
  const result = z.object({ request: z.unknown() }).strict().safeParse(args);
  if (!result.success) error('profile_invalid', operation, 'Invalid request arguments');
  const request = schema.safeParse(result.data!.request);
  if (!request.success) error('profile_invalid', operation, 'Invalid request arguments');
  return request.data!;
}
function draftArguments(args: unknown, operation: string): ProfileDraft {
  const result = z.object({ draft: profileDraftSchema }).strict().safeParse(args);
  if (!result.success) error('profile_invalid', operation, 'Invalid draft arguments');
  return result.data!.draft;
}
function validateId(args: unknown, operation: string) { return requestArguments(profileIdRequestSchema, args, operation).profileId; }
export const mockBackend = {
  reset() { profiles = []; drafts = new Map(); runtime = { state: 'disconnected' }; statuses = new Map(); inventoryGeneration = 0; inventoryObservedAt = null; inventoryContainerState = 'running'; autoRegistrationEnabled = false; backup = null; backupDrafts.clear(); registryRevision = 0; storage()?.removeItem(storageKey); overrides.clear(); errors.clear(); invocations = []; nextId = 1; },
  getInvocations() { return [...invocations]; },
  setResponseOverride(command: string, value: unknown) { overrides.set(command, value); },
  setErrorOverride(command: string, value: unknown) { errors.set(command, value); },
  async invoke(command: string, args?: unknown): Promise<unknown> {
    invocations.push({ command, args }); publishInvocations();
    if (errors.has(command)) throw errors.get(command);
    if (overrides.has(command)) return overrides.get(command);
    switch (command) {
      case 'list_profiles': if (args !== undefined) error('profile_invalid', command, 'unexpected arguments'); return profiles;
      case 'get_profile': { const profileId = validateId(args, command); const profile = profiles.find(p => p.id === profileId); if (!profile) error('profile_not_found', command, 'Profile not found', profileId); const draft = drafts.get(profileId)!; return { profile, composeFiles: draft.composeFiles, environmentFiles: draft.environmentFiles }; }
      case 'inspect_profile_draft': { const draft = draftArguments(args, command); try { validateDraft(draft, command); return { valid: true, issues: [] }; } catch (caught) { return { valid: false, issues: [{ message: (caught as AppErrorException).message }] }; } }
        case 'create_profile': { const draft = draftArguments(args, command); validateDraft(draft, command); const profileId = id(); if (typeof window !== 'undefined') window.__COLUI_MOCK_PROFILE_ID = profileId; const profile: ProfileSummary = { id: profileId, revision: 1, displayName: draft.displayName, composeProjectName: draft.composeProjectName, workingDirectory: draft.workingDirectory, registrationOrigin: 'manual' }; profiles = [...profiles, profile]; drafts.set(profileId, draft); statuses.set(profileId, baseStatus(profileId)); persist(); return profile; }
       case 'update_profile': { const { profileId, expectedRevision, patch } = requestArguments(updateProfileRequestSchema, args, command); validateDraft(patch, command); const profile = profiles.find(p => p.id === profileId); if (!profile) error('profile_not_found', command, 'Profile not found', profileId); if (profile!.revision !== expectedRevision) error('profile_revision_conflict', command, 'Profile revision conflict', profileId); const updated: ProfileSummary = { ...profile!, revision: profile!.revision + 1, displayName: patch.displayName, composeProjectName: patch.composeProjectName, workingDirectory: patch.workingDirectory }; profiles = profiles.map(p => p.id === profileId ? updated : p); drafts.set(profileId, patch); persist(); return updated; }
       case 'remove_profile': { const request = requestArguments(removeProfileRequestSchema, args, command); const profile = profiles.find(p => p.id === request.profileId); if (!profile) error('profile_not_found', command, 'Profile not found', request.profileId); if (profile!.revision !== request.expectedRevision) error('profile_revision_conflict', command, 'Profile revision conflict', profile!.id); profiles = profiles.filter(p => p.id !== profile!.id); drafts.delete(profile!.id); statuses.delete(profile!.id); persist(); return undefined; }
      case 'get_runtime_state': return runtime;
      case 'connect_runtime': if (typeof window !== 'undefined' && window.location.search.includes('runtimeFailure')) error('runtime_connection_failed', command, 'runtime offline'); runtime = { state: 'ready', context: { sessionId: id(), endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: now() } }; return runtime;
      case 'get_inventory': return inventory();
      case 'refresh_inventory': return publishInventory();
      case 'list_discovery_candidates': return { candidates: [], autoRegistrationEnabled };
      case 'get_auto_registration_configuration': return { enabled: autoRegistrationEnabled };
      case 'configure_auto_registration': { const request = s.configureAutoRegistrationRequestSchema.parse((args as { request: unknown }).request); autoRegistrationEnabled = request.enabled; return { enabled: autoRegistrationEnabled }; }
      case 'ignore_candidate': s.ignoreCandidateRequestSchema.parse((args as { request: unknown }).request); return false;
      case 'register_candidate': { const request = s.registerCandidateRequestSchema.parse((args as { request: unknown }).request); const profileId = id(); const draft = { displayName: request.composeProjectName, composeProjectName: request.composeProjectName, workingDirectory: request.workingDirectory ?? '', composeFiles: request.configFiles, environmentFiles: [] }; const profile: ProfileSummary = { id: profileId, revision: 1, displayName: draft.displayName, composeProjectName: draft.composeProjectName, workingDirectory: draft.workingDirectory, registrationOrigin: 'discovered' }; profiles = [...profiles, profile]; drafts.set(profileId, draft); persist(); return profile; }
      case 'auto_register_candidates': s.autoRegistrationRequestSchema.parse((args as { request: unknown }).request); return { profiles: [], errors: [] };
      case 'disconnect_runtime': runtime = { state: 'disconnected' }; return runtime;
      case 'reconnect_runtime': runtime = { state: 'ready', context: { sessionId: id(), endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: now() } }; return { runtimeState: runtime, inventory: publishInventory() };
      case 'create_registry_backup': backup = [...profiles]; backupDrafts = new Map(drafts); return registryIdentity();
      case 'restore_registry_backup': if (backup === null) error('recovery_conflict', command, 'No backup'); profiles = [...backup!]; drafts = new Map(backupDrafts); persist(); return profiles;
      case 'run_container_action': { const request = s.containerActionRequestSchema.parse((args as { request: unknown }).request); return { containerId: request.containerId, action: request.action, observation: 'confirmed_in_session', inventory: publishInventory(request.action === 'stop' ? 'stopped' : 'running') }; }
      case 'get_container_logs': { const request = s.containerLogsRequestSchema.parse((args as { request: unknown }).request); return { containerId: request.containerId, text: 'Mock container logs\n', retainedBytes: 20, truncated: false, observedAt: now() }; }
      case 'open_container_port': s.openContainerPortRequestSchema.parse((args as { request: unknown }).request); return null;
      case 'get_diagnostics': return { runtime: { state: runtime, resolvedEndpoint: runtime.state === 'ready' ? runtime.context.endpoint : null, apiFingerprint: runtime.state === 'ready' ? runtime.context.daemonFingerprint : null, cliFingerprint: runtime.state === 'ready' ? runtime.context.daemonFingerprint : null, sessionId: runtime.state === 'ready' ? runtime.context.sessionId : null, connectedAt: runtime.state === 'ready' ? runtime.context.connectedAt : null }, registry: { registryPath: '/mock/registry.json', backupPath: '/mock/registry.backup.json', backup: { exists: backup !== null, modifiedAt: null, state: backup === null ? 'missing' : 'valid', error: null }, revision: registryRevision, health: { state: 'healthy', identity: registryIdentity(), error: null, lastOperationAt: null, lastFailureAt: null }, lockTimeoutMs: 500, lastRecoveryResult: null }, import: { sourcePath: '/mock/legacy.json', importedCount: 0, sourcePreserved: true, error: null }, operations: { generation: 0, active: [] }, inventory: inventory(), definitions: { generation: 0, profiles: [] }, journal: { entries: [] } };
      case 'get_project_details': {
        const profileId = validateId(args, command);
        const profile = profiles.find(p => p.id === profileId);
        if (!profile) error('profile_not_found', command, 'Profile not found', profileId);
        const key = profileAssociationKey(profile!);
        const profileAssociationUnique = key !== null && profiles.filter(p => profileAssociationKey(p) === key).length === 1;
        return {
          profile: { profile, composeFiles: drafts.get(profileId)!.composeFiles, environmentFiles: drafts.get(profileId)!.environmentFiles },
          definition: { profileId, definitionRevision: null, loadedAt: null, state: 'unchecked', services: [], issues: [], error: null },
          runtime: profileAssociationUnique ? baseStatus(profileId).runtime : { presence: inventoryGeneration > 0 ? 'absent' : 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null },
          runtimeIssues: profileAssociationUnique ? [] : [{ field: 'ambiguous_runtime_association', message: 'runtime association is ambiguous' }],
          profileAssociationUnique,
        };
      }
      case 'refresh_project_definition': { const profileId = validateId(args, command); if (!profiles.some(p => p.id === profileId)) error('profile_not_found', command, 'Profile not found', profileId); return { profileId, definitionRevision: 'mock-definition', loadedAt: now(), state: 'valid', services: [], issues: [], error: null }; }
      case 'get_project_status': { const profileId = validateId(args, command); if (!profiles.some(p => p.id === profileId)) error('profile_not_found', command, 'Profile not found', profileId); return statuses.get(profileId) ?? baseStatus(profileId); }
        case 'apply_project': case 'stop_project': case 'tear_down_project': case 'restart_project': { const profileId = validateId(args, command); if (!profiles.some(p => p.id === profileId)) error('profile_not_found', command, 'Profile not found', profileId); const activity = command === 'stop_project' || command === 'tear_down_project' ? 'none-running' : 'all-running'; const kind = command.replace('_project', '').replace('_', '-') as 'apply' | 'stop' | 'tear-down' | 'restart'; const status = { ...(statuses.get(profileId) as object ?? baseStatus(profileId)), runtime: { presence: 'present', activity, containerCount: 1, runningContainerCount: activity === 'none-running' ? 0 : 1, observedAt: now() }, operation: { kind, phase: 'succeeded', startedAt: now() } }; statuses.set(profileId, status); const refreshed = publishInventory(activity === 'none-running' ? 'stopped' : 'running'); return { profileId, success: true, inventoryGeneration: refreshed.generation, inventory: refreshed }; }
      default: error('protocol_mismatch', command, `Unknown command: ${command}`);
    }
  },
};
