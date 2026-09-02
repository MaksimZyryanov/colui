import { describe, expect, it } from 'vitest';
import { profileSummarySchema, runtimeStateSchema } from '../schemas';
import { decodeResponse } from '../validation';

describe('IPC contracts', () => {
  it('turns malformed success payload into protocol_mismatch', () => {
    const raw = { revision: 1, displayName: 'Missing id' };
    expect(() => decodeResponse(profileSummarySchema, raw, 'list_profiles'))
      .toThrowError(expect.objectContaining({ code: 'protocol_mismatch' }));
  });

  it('accepts committed tagged runtime state shape', () => {
    expect(runtimeStateSchema.parse({ state: 'disconnected' })).toEqual({ state: 'disconnected' });
  });
});
