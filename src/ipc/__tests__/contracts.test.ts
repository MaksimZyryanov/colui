import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { appErrorSchema, errorCodeSchema, appErrorSubjectSchema, appErrorSubjectKindSchema } from '../errors';
import { daemonFingerprintSchema, lifecycleResultSchema, mismatchDetailsSchema, profileDetailsSchema, profileDraftSchema, profileIdRequestSchema, profileSummarySchema, profileValidationSchema, projectStatusSchema, runtimeProjectionSchema, runtimeStateSchema, sessionContextSchema } from '../schemas';
import * as s from '../schemas';
import { decodeResponse } from '../validation';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const fixtureRoot = resolve('schemas/fixtures');
const dtoManifest = [
  ['AppErrorCodeDto', errorCodeSchema], ['AppErrorDto', appErrorSchema], ['IssueDto', s.issueSchema],
  ['RegistrationOriginDto', s.registrationOriginSchema], ['ProfileSummaryDto', profileSummarySchema], ['ProfileDraftDto', profileDraftSchema],
  ['ProfilePatchDto', s.profilePatchSchema], ['ProfileDetailsDto', profileDetailsSchema], ['ProfileValidationDto', profileValidationSchema],
  ['ProfileIdRequestDto', profileIdRequestSchema], ['UpdateProfileRequestDto', s.updateProfileRequestSchema], ['RemoveProfileRequestDto', s.removeProfileRequestSchema],
  ['DaemonFingerprintDto', daemonFingerprintSchema], ['SessionContextDto', sessionContextSchema], ['MismatchDetailsDto', mismatchDetailsSchema],
  ['RuntimePresenceDto', z.enum(['unavailable', 'absent', 'present'])], ['RuntimeActivityDto', z.enum(['all-running', 'mixed', 'none-running'])],
  ['DefinitionStateDto', z.enum(['unchecked', 'valid', 'invalid', 'stale'])], ['OperationKindDto', z.enum(['apply', 'stop', 'tear-down', 'restart'])],
  ['OperationPhaseDto', z.enum(['queued', 'running', 'succeeded', 'failed'])], ['RuntimeProjectionDto', runtimeProjectionSchema],
  ['DefinitionProjectionDto', z.object({ state: z.enum(['unchecked', 'valid', 'invalid', 'stale']), revision: z.string().nullable().optional(), serviceCount: z.number().int().nonnegative().nullable().optional() })],
  ['OperationDto', z.object({ kind: z.enum(['apply', 'stop', 'tear-down', 'restart']), phase: z.enum(['queued', 'running', 'succeeded', 'failed']), startedAt: z.string().datetime({ offset: true }) })],
  ['ProjectStatusDto', projectStatusSchema], ['RuntimeStateDto', runtimeStateSchema], ['LifecycleResultDto', lifecycleResultSchema], ['RuntimeInventoryDto', s.inventorySchema], ['ProjectDefinitionDto', s.projectDefinitionSchema], ['ProjectDetailsResponseDto', s.projectDetailsResponseSchema],
  ['AppErrorSubjectDto', appErrorSubjectSchema], ['AppErrorSubjectKindDto', appErrorSubjectKindSchema],
  ['ContainerStateDto', s.containerStateSchema], ['PortBindingDto', s.portBindingSchema], ['PortBindingActionDto', s.portBindingActionSchema], ['ContainerInstanceDto', s.containerInstanceSchema], ['ProjectRuntimeSnapshotDto', s.projectRuntimeSnapshotSchema], ['ComposeObservationGroupDto', s.composeObservationGroupSchema], ['InventoryFreshnessDto', s.inventoryFreshnessSchema], ['ServiceDefinitionDto', s.serviceDefinitionSchema],
  ['DiscoveryClassificationDto', s.discoveryClassificationSchema], ['DiscoveryConflictSourceDto', s.discoveryConflictSourceSchema], ['DiscoveryConflictEvidenceDto', s.discoveryConflictEvidenceSchema], ['DiscoveryCandidateDto', s.discoveryCandidateSchema], ['DiscoveryListDto', s.discoveryListSchema], ['RegisterCandidateRequestDto', s.registerCandidateRequestSchema], ['IgnoreCandidateRequestDto', s.ignoreCandidateRequestSchema], ['ConfigureAutoRegistrationRequestDto', s.configureAutoRegistrationRequestSchema], ['AutoRegistrationConfigurationDto', s.autoRegistrationConfigurationSchema], ['AutoRegistrationResultDto', s.autoRegistrationResultSchema], ['AutoRegistrationRequestDto', s.autoRegistrationRequestSchema],
  ['RegistrySnapshotIdentityDto', s.registrySnapshotIdentitySchema], ['RegistryHealthStateDto', s.registryHealthStateSchema], ['RegistryHealthDto', s.registryHealthSchema], ['RuntimeDiagnosticsDto', s.runtimeDiagnosticsSchema], ['RecoveryResultDto', s.recoveryResultSchema], ['RegistryDiagnosticsDto', s.registryDiagnosticsSchema], ['BackupValidationStateDto', s.backupValidationStateSchema], ['RegistryBackupDiagnosticsDto', s.registryBackupDiagnosticsSchema], ['ImportDiagnosticsDto', s.importDiagnosticsSchema], ['ActiveOperationPhaseDto', s.activeOperationPhaseSchema], ['ActiveOperationDto', s.activeOperationSchema], ['OperationsDiagnosticsDto', s.operationsDiagnosticsSchema], ['ProfileDefinitionDiagnosticsDto', s.profileDefinitionDiagnosticsSchema], ['DefinitionsDiagnosticsDto', s.definitionsDiagnosticsSchema], ['JournalEventKindDto', s.journalEventKindSchema], ['JournalSeverityDto', s.journalSeveritySchema], ['JournalEntryDto', s.journalEntrySchema], ['SessionJournalDto', s.sessionJournalSchema], ['DiagnosticsSnapshotDto', s.diagnosticsSnapshotSchema], ['ApplicationStateScopeDto', s.applicationStateScopeSchema], ['ApplicationStateChangedDto', s.applicationStateChangedSchema],
  ['ContainerActionDto', s.containerActionSchema], ['ContainerActionObservationDto', s.containerActionObservationSchema], ['ContainerActionRequestDto', s.containerActionRequestSchema], ['ContainerActionResultDto', s.containerActionResultSchema], ['ContainerLogsRequestDto', s.containerLogsRequestSchema], ['ContainerLogsDto', s.containerLogsSchema], ['OpenContainerPortRequestDto', s.openContainerPortRequestSchema], ['ReconnectResultDto', s.reconnectResultSchema],
] as const;
const fixtureManifest: Array<[string, { safeParse: (value: unknown) => { success: boolean } }, boolean]> = [
  ['app_error_invalid_code.json', appErrorSchema, false], ['app_error_missing_retryable.json', appErrorSchema, false],
  ['inventory_pre_observation_invalid_snapshot.json', s.inventorySchema, false], ['inventory_pre_observation_valid.json', s.inventorySchema, true],
  ['lifecycle_result_invalid_missing_success.json', lifecycleResultSchema, false], ['lifecycle_result_valid.json', lifecycleResultSchema, true],
  ['profile_details_invalid_missing_required.json', profileDetailsSchema, false], ['profile_summary_invalid_enum.json', profileSummarySchema, false],
  ['profile_summary_invalid_missing_id.json', profileSummarySchema, false], ['profile_summary_invalid_uuid.json', profileSummarySchema, false], ['profile_summary_valid.json', profileSummarySchema, true],
  ['profile_validation_valid.json', profileValidationSchema, true], ['project_status_invalid_runtime_combination.json', projectStatusSchema, false], ['project_status_runtime_unavailable.json', projectStatusSchema, true],
  ['request_invalid_extra_field.json', profileIdRequestSchema.strict(), false], ['runtime_context_mismatch.json', runtimeStateSchema, true], ['runtime_unavailable.json', runtimeStateSchema, true],
  ['session_context_invalid_timestamp.json', sessionContextSchema, false],
];

const normalizeSchema = (value: unknown, root = value): unknown => {
  if (Array.isArray(value)) return value.map(child => normalizeSchema(child, root));
  if (!value || typeof value !== 'object') return value;
  const object = value as Record<string, unknown>;
  if (typeof object.$ref === 'string' && object.$ref.startsWith('#/definitions/')) {
    const definition = (root as Record<string, unknown>).definitions as Record<string, unknown> | undefined;
    const target = definition?.[object.$ref.slice('#/definitions/'.length)];
    if (target) return normalizeSchema(target, root);
  }
  const normalized = Object.fromEntries(Object.entries(object)
    .filter(([key]) => !['$schema', 'title', 'description'].includes(key))
    .filter(([key]) => key !== 'definitions')
    .map(([key, child]) => [key, normalizeSchema(child, root)]));
  if (normalized.type === 'object' && normalized.additionalProperties === undefined) normalized.additionalProperties = false;
  if (normalized.const !== undefined) {
    normalized.enum = [normalized.const];
    delete normalized.const;
  }
  if (normalized.anyOf !== undefined) {
    normalized.oneOf = normalized.anyOf;
    delete normalized.anyOf;
  }
  if (Array.isArray(normalized.oneOf) && normalized.oneOf.every(item => item && typeof item === 'object' && Object.keys(item as object).every(key => key === 'type' || key === 'format' || key === 'minimum'))) {
    const branches = normalized.oneOf as Array<Record<string, unknown>>;
    if (branches.every(item => item.type === 'null' || item.type === 'string' || item.type === 'integer')) {
      normalized.type = branches.map(item => item.type).sort();
      const formatted = branches.find(item => item.format !== undefined);
      const bounded = branches.find(item => item.minimum !== undefined);
      if (formatted) normalized.format = formatted.format;
      if (bounded) normalized.minimum = bounded.minimum;
      delete normalized.oneOf;
    }
  }
  if (typeof normalized.format === 'string' && normalized.format.startsWith('uint')) delete normalized.format;
  for (const key of ['required', 'properties', 'definitions']) {
    const child = normalized[key];
    if (Array.isArray(child)) normalized[key] = [...child].sort();
    if (child && typeof child === 'object' && !Array.isArray(child)) normalized[key] = Object.fromEntries(Object.entries(child).sort(([a], [b]) => a.localeCompare(b)));
  }
  return normalized;
};

describe('IPC contracts', () => {
  it('decodes pre-observation generation zero only with hasSnapshot false', () => {
    const preObservationFixture = {
      generation: 0,
      hasSnapshot: false,
      observedAt: null,
      runtimeSessionId: null,
      daemonFingerprint: null,
      freshness: 'unavailable',
      lastSuccessfulObservedAt: null,
      containers: [],
      projects: [],
      composeObservationGroups: [],
      standaloneContainers: [],
      error: null,
    };
    expect(s.inventorySchema.parse(preObservationFixture).hasSnapshot).toBe(false);
    expect(s.inventorySchema.safeParse({ ...preObservationFixture, hasSnapshot: true }).success).toBe(false);
  });

  it('turns malformed success payload into protocol_mismatch', () => {
    const raw = { revision: 1, displayName: 'Missing id' };
    expect(() => decodeResponse(profileSummarySchema, raw, 'list_profiles'))
      .toThrowError(expect.objectContaining({ code: 'protocol_mismatch' }));
  });

  it('accepts committed tagged runtime state shape', () => {
    expect(runtimeStateSchema.parse({ state: 'disconnected' })).toEqual({ state: 'disconnected' });
  });

  it('requires Rust DTO fields while accepting nullable values', () => {
    expect(projectStatusSchema.safeParse({ profileId: '00000000-0000-0000-0000-000000000001', runtime: { containerCount: 0, runningContainerCount: 0 }, definition: { state: 'unchecked' }, issues: [] }).success).toBe(false);
    expect(projectStatusSchema.safeParse({ profileId: '00000000-0000-0000-0000-000000000001', runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [] }).success).toBe(true);
  });

  it('rejects invalid runtime status combination', () => {
    expect(projectStatusSchema.safeParse(JSON.parse(readFileSync(resolve('schemas/fixtures/project_status_invalid_runtime_combination.json'), 'utf8'))).success).toBe(false);
  });

  it('accepts valid RFC3339 session context', () => {
    expect(sessionContextSchema.safeParse({ sessionId: '00000000-0000-0000-0000-000000000001', endpoint: 'x', daemonFingerprint: { daemonId: 'd', serverVersion: '1', osType: 'x', architecture: 'x' }, connectedAt: '2026-09-02T00:00:00Z' }).success).toBe(true);
  });

  it('matches Rust optional error fields and u64 zero', () => {
    expect(appErrorSchema.parse({ code: 'profile_invalid', operation: 'x', message: 'bad', retryable: false })).toEqual({ code: 'profile_invalid', operation: 'x', message: 'bad', retryable: false });
    expect(profileSummarySchema.parse({ id: '00000000-0000-0000-0000-000000000001', revision: 0, displayName: '', composeProjectName: '', workingDirectory: '', registrationOrigin: 'manual' }).revision).toBe(0);
  });

  it('decodes retained definition and typed error in one projection', () => {
    const projection = s.projectDefinitionSchema.parse({
      profileId: '00000000-0000-0000-0000-000000000001', definitionRevision: 'retained',
      loadedAt: '2026-09-03T00:00:00Z', state: 'stale', services: [{ name: 'web', image: 'nginx', buildContext: null, declaredPorts: [] }], issues: [],
      error: { code: 'definition_failed', operation: 'definition', subject: { kind: 'profile', id: '00000000-0000-0000-0000-000000000001' }, message: 'compose config failed', details: null, retryable: false },
    });
    expect(projection.services[0].name).toBe('web');
    expect(projection.error?.code).toBe('definition_failed');
  });

  it('validates both inventory observation fixtures and generation marker invariants', () => {
    const valid = JSON.parse(readFileSync(resolve('schemas/fixtures/inventory_pre_observation_valid.json'), 'utf8'));
    const invalid = JSON.parse(readFileSync(resolve('schemas/fixtures/inventory_pre_observation_invalid_snapshot.json'), 'utf8'));
    expect(s.inventorySchema.safeParse(valid).success).toBe(true);
    expect(s.inventorySchema.safeParse(invalid).success).toBe(false);
    expect(s.inventorySchema.safeParse({ ...valid, generation: 1, hasSnapshot: false }).success).toBe(false);
    expect(s.inventorySchema.safeParse({ ...valid, generation: 0, hasSnapshot: true }).success).toBe(false);
  });

  it('accepts Rust DTO empty strings and omitted optional projections', () => {
    expect(profileDraftSchema.parse({ displayName: '', composeProjectName: '', workingDirectory: '', composeFiles: [], environmentFiles: [] })).toBeTruthy();
    expect(projectStatusSchema.parse({ profileId: '00000000-0000-0000-0000-000000000001', runtime: { presence: 'unavailable', containerCount: 0, runningContainerCount: 0 }, definition: { state: 'unchecked' }, issues: [] })).toBeTruthy();
  });

  it('bounds and describes protocol mismatch details', () => {
    try { decodeResponse(profileSummarySchema, { bad: 'x'.repeat(1000) }, 'list_profiles'); } catch (error) {
      expect(error).toMatchObject({ code: 'protocol_mismatch' });
      expect((error as Error).message).toContain('list_profiles');
      expect((error as { details: string }).details.length).toBeLessThanOrEqual(500);
    }
  });

  it('accepts valid committed DTO fixtures and rejects invalid ones', () => {
    const fixture = (name: string) => JSON.parse(readFileSync(resolve('schemas/fixtures', name), 'utf8'));
    expect(decodeResponse(profileSummarySchema, fixture('profile_summary_valid.json'), 'fixture')).toBeTruthy();
    expect(() => decodeResponse(profileSummarySchema, fixture('profile_summary_invalid_uuid.json'), 'fixture')).toThrow();
    expect(() => decodeResponse(profileSummarySchema, fixture('profile_summary_invalid_enum.json'), 'fixture')).toThrow();
    expect(() => decodeResponse(profileSummarySchema, fixture('profile_summary_invalid_missing_id.json'), 'fixture')).toThrow();
    expect(decodeResponse(runtimeStateSchema, fixture('runtime_unavailable.json'), 'fixture')).toMatchObject({ state: 'failed' });
  });

  it('preserves unknown-field behavior at IPC boundary', () => {
    expect(profileIdRequestSchema.safeParse({ profileId: '00000000-0000-0000-0000-000000000001', extra: true }).success).toBe(false);
    expect(s.profileIdRequestSchema.strict().safeParse({ profileId: '00000000-0000-0000-0000-000000000001', extra: true }).success).toBe(false);
  });

  it('checks every committed fixture against its corresponding DTO schema', () => {
    const fixture = (name: string) => JSON.parse(readFileSync(resolve('schemas/fixtures', name), 'utf8'));
    const cases: Array<[string, { safeParse: (value: unknown) => { success: boolean } }, boolean]> = [
      ['profile_summary_valid.json', profileSummarySchema, true],
      ['profile_summary_invalid_uuid.json', profileSummarySchema, false],
      ['profile_summary_invalid_enum.json', profileSummarySchema, false],
      ['profile_summary_invalid_missing_id.json', profileSummarySchema, false],
      ['profile_details_invalid_missing_required.json', profileDetailsSchema, false],
      ['profile_validation_valid.json', profileValidationSchema, true],
      ['runtime_unavailable.json', runtimeStateSchema, true],
      ['runtime_context_mismatch.json', runtimeStateSchema, true],
      ['session_context_invalid_timestamp.json', sessionContextSchema, false],
      ['project_status_runtime_unavailable.json', projectStatusSchema, true],
      ['project_status_invalid_runtime_combination.json', projectStatusSchema, false],
      ['lifecycle_result_valid.json', lifecycleResultSchema, true],
    ];
    for (const [name, schema, expected] of cases) expect(schema.safeParse(fixture(name)).success, name).toBe(expected);
  });

  it('covers standalone committed DTO shapes and request validation', () => {
    const id = '00000000-0000-0000-0000-000000000001';
    expect(daemonFingerprintSchema.safeParse({ daemonId: 'd', serverVersion: '1', osType: 'x', architecture: 'x' }).success).toBe(true);
    expect(mismatchDetailsSchema.safeParse({ endpoint: 'x', apiFingerprint: { daemonId: 'd', serverVersion: '1', osType: 'x', architecture: 'x' }, cliFingerprint: { daemonId: 'd', serverVersion: '1', osType: 'x', architecture: 'x' } }).success).toBe(true);
    expect(runtimeProjectionSchema.safeParse({ presence: 'present', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }).success).toBe(true);
    expect(profileIdRequestSchema.safeParse({ profileId: id }).success).toBe(true);
    expect(profileIdRequestSchema.safeParse({ profileId: 'id-1' }).success).toBe(false);
    expect(profileIdRequestSchema.safeParse({ profileId: '00000000000000000000000000000001' }).success).toBe(false);
    expect(profileIdRequestSchema.safeParse({ profileId: 'abcdefab-cdef-abcd-efab-cdefabcdefab' }).success).toBe(true);
    expect(profileIdRequestSchema.safeParse({ profileId: 'ABCDEFAB-CDEF-ABCD-EFAB-CDEFABCDEFAB' }).success).toBe(false);
  });

  it('compares every TypeScript DTO against generated Rust schema semantically', async () => {
    const { zodToJsonSchema } = await import('zod-to-json-schema');
    for (const [name, schema] of dtoManifest) {
      const rust = JSON.parse(readFileSync(resolve('schemas', `${name}.json`), 'utf8'));
      const generated = zodToJsonSchema(schema, { name, $refStrategy: 'none' });
      expect(normalizeSchema(generated), name).toEqual(normalizeSchema(rust));
    }
   expect(dtoManifest.map(([name]) => `${name}.json`).sort()).toEqual(readdirSync(resolve('schemas')).filter(name => name.endsWith('.json')).sort());
  });

  it('does not discard additionalProperties contract metadata', () => {
    expect(normalizeSchema({ type: 'object', additionalProperties: false })).toEqual({ type: 'object', additionalProperties: false });
    expect(normalizeSchema({ type: 'object' })).toEqual({ type: 'object', additionalProperties: false });
    expect(normalizeSchema({ type: 'object', additionalProperties: true })).toEqual({ type: 'object', additionalProperties: true });
  });

  it('preserves typed subjects and retryability without pretending container IDs are profile UUIDs', () => {
    for (const kind of ['profile', 'candidate', 'container', 'registry']) {
      const raw = { code: 'operation_conflict', operation: 'action', subject: { kind, id: 'opaque-id' }, message: 'Busy', retryable: true };
      expect(decodeResponse(appErrorSchema, raw, 'action')).toEqual(raw);
    }
    expect(appErrorSchema.safeParse({ code: 'candidate_stale', operation: 'register', message: 'Stale', retryable: false, subject: { kind: 'daemon', id: 'x' } }).success).toBe(false);
  });

  it('validates discovery identity, session, enums and strict immutable requests', () => {
    const candidate = { candidateId: 'a'.repeat(64), runtimeSessionId: '00000000-0000-0000-0000-000000000001', inventoryGeneration: 1, composeProjectName: 'demo', workingDirectory: null, configFiles: [], containerCount: 1, classification: 'incomplete_metadata', conflicts: [], ignored: false };
    expect(s.discoveryCandidateSchema.parse(candidate)).toEqual(candidate);
    for (const patch of [{ candidateId: 'A'.repeat(64) }, { runtimeSessionId: 'not-a-session' }, { inventoryGeneration: -1 }, { classification: 'new' }]) {
      expect(() => decodeResponse(s.discoveryCandidateSchema, { ...candidate, ...patch }, 'list_discovery_candidates')).toThrowError(expect.objectContaining({ code: 'protocol_mismatch', retryable: false }));
    }
    expect(s.registrySnapshotIdentitySchema.safeParse({ registryRevision: 0, canonicalContentSha256: 'a'.repeat(64) }).success).toBe(true);
    expect(s.registrySnapshotIdentitySchema.safeParse({ registryRevision: 1, canonicalContentSha256: 'bad' }).success).toBe(false);
    const request = { containerId: 'opaque', runtimeSessionId: candidate.runtimeSessionId };
    expect(s.containerLogsRequestSchema.parse(request)).toEqual(request);
    expect(s.containerLogsRequestSchema.safeParse({ ...request, path: '/tmp/log' }).success).toBe(false);
    expect(s.containerActionRequestSchema.safeParse({ ...request, action: 'delete' }).success).toBe(false);
    expect(s.containerLogsSchema.safeParse({ containerId: 'opaque', text: '', retainedBytes: 262145, truncated: true, observedAt: '2026-09-04T00:00:00Z' }).success).toBe(false);
  });

  it('requires observation evidence and backend port actions in inventory', () => {
    const inventory = JSON.parse(readFileSync(resolve(fixtureRoot, 'inventory_pre_observation_valid.json'), 'utf8'));
    expect(s.inventorySchema.safeParse({ ...inventory, composeObservationGroups: undefined }).success).toBe(false);
    expect(s.inventorySchema.safeParse({ ...inventory, composeObservationGroups: [{ composeProjectName: 'demo', workingDirectory: null, configFiles: [], containerIds: ['id'] }] }).success).toBe(false);
    expect(s.portBindingSchema.safeParse({ containerPort: 80, protocol: 'tcp' }).success).toBe(false);
    expect(s.portBindingSchema.parse({ containerPort: 80, protocol: 'tcp', action: { copy: '80/tcp', url: null } }).action.url).toBeNull();
  });

  it('exercises every committed fixture exactly once', () => {
    const fixture = (name: string) => JSON.parse(readFileSync(resolve(fixtureRoot, name), 'utf8'));
    expect(fixtureManifest.map(([name]) => name).sort()).toEqual(readdirSync(fixtureRoot).sort());
    for (const [name, schema, expected] of fixtureManifest) expect(schema.safeParse(fixture(name)).success, name).toBe(expected);
  });
});
