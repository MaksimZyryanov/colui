import { z } from 'zod';

export const errorCodeSchema = z.enum(['runtime_unavailable','runtime_connection_failed','runtime_context_mismatch','profile_not_found','profile_already_registered','profile_revision_conflict','profile_invalid','definition_failed','compose_failed','container_operation_failed','operation_conflict','operation_timeout','registry_corrupt','registry_locked','registry_write_failed','permission_denied','protocol_mismatch']);
export const appErrorSchema = z.object({ code: errorCodeSchema, operation: z.string(), subjectId: z.string().uuid().nullable(), message: z.string(), details: z.string().nullable(), retryable: z.boolean() });
export type AppError = z.infer<typeof appErrorSchema>;
export class AppErrorException extends Error implements AppError {
  code!: AppError['code']; operation!: string; subjectId!: string | null; details!: string | null; retryable!: boolean;
  constructor(error: AppError) { super(error.message); Object.assign(this, error); }
}
export function normalizeError(raw: unknown, operation: string): AppErrorException {
  const parsed = appErrorSchema.safeParse(raw);
  if (parsed.success) return new AppErrorException(parsed.data);
  const detail = typeof raw === 'string' ? raw.slice(0, 500) : 'Unknown transport failure';
  return new AppErrorException({ code: 'runtime_connection_failed', operation, subjectId: null, message: `IPC operation failed: ${operation}`, details: detail, retryable: true });
}
