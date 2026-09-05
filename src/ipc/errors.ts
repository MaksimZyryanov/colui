import { z } from 'zod';
export const appErrorSubjectKindSchema = z.enum(['profile', 'candidate', 'container', 'registry']);
export const appErrorSubjectSchema = z.object({ kind: appErrorSubjectKindSchema, id: z.string() }).strict();

export const errorCodeSchema = z.enum(['runtime_unavailable','runtime_connection_failed','runtime_context_mismatch','candidate_stale','discovery_conflict','profile_not_found','profile_already_registered','profile_revision_conflict','profile_invalid','definition_failed','compose_failed','container_operation_failed','operation_conflict','operation_timeout','registry_corrupt','registry_locked','registry_write_failed','recovery_conflict','permission_denied','protocol_mismatch']);
export const appErrorSchema = z.object({ code: errorCodeSchema, operation: z.string(), subject: appErrorSubjectSchema.nullable().optional(), message: z.string(), details: z.string().nullable().optional(), retryable: z.boolean() });
export type AppError = z.infer<typeof appErrorSchema>;
export class AppErrorException extends Error implements AppError {
  code!: AppError['code']; operation!: string; subject?: AppError['subject']; details?: string | null; retryable!: boolean;
  constructor(error: AppError) { super(error.message); Object.assign(this, error); }
}
export function normalizeError(raw: unknown, operation: string): AppErrorException {
  let candidate = raw;
  if (typeof raw === 'string') {
    try { candidate = JSON.parse(raw); } catch { /* raw transport text */ }
  }
  const parsed = appErrorSchema.safeParse(candidate);
  if (parsed.success) return new AppErrorException(parsed.data);
  let detail = typeof raw === 'string' ? raw : 'Unknown transport failure';
  if (raw instanceof Error) detail = raw.message;
  else if (typeof raw === 'object' && raw !== null) {
    try { detail = JSON.stringify(raw); } catch { detail = 'Unserializable transport failure'; }
  }
  detail = detail.slice(0, 500);
  return new AppErrorException({ code: 'runtime_connection_failed', operation, subject: null, message: `IPC operation failed: ${operation}`, details: detail, retryable: true });
}
