import { z } from 'zod';
import { AppErrorException } from './errors';
export function decodeResponse<T>(schema: z.ZodType<T, z.ZodTypeDef, unknown>, raw: unknown, operation: string): T { const result = schema.safeParse(raw); if (result.success) return result.data; const details = result.error.issues.slice(0, 5).map(i => `${i.path.join('.') || '<root>'}: ${i.message}`).join('; ').slice(0, 500); throw new AppErrorException({code:'protocol_mismatch',operation,subject:null,message:`Invalid response from ${operation}`,details,retryable:false}); }
