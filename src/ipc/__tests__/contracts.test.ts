import { describe, expect, it } from 'vitest';
import { appErrorSchema } from '../errors';
import { profileDraftSchema, profileSummarySchema, projectStatusSchema, runtimeStateSchema } from '../schemas';
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
});
