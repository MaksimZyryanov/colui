import { describe, expect, it } from 'vitest';
import { appErrorSchema } from '../errors';
import { daemonFingerprintSchema, lifecycleResultSchema, mismatchDetailsSchema, profileDetailsSchema, profileDraftSchema, profileIdRequestSchema, profileSummarySchema, profileValidationSchema, projectStatusSchema, runtimeProjectionSchema, runtimeStateSchema, sessionContextSchema } from '../schemas';
import { decodeResponse } from '../validation';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('IPC contracts', () => {
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
});
