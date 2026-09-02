import { z } from 'zod';
import { AppErrorException } from './errors';
export function decodeResponse<T>(schema: z.ZodType<T>, raw: unknown, operation: string): T { const result = schema.safeParse(raw); if (result.success) return result.data; throw new AppErrorException({code:'protocol_mismatch',operation,subjectId:null,message:`Invalid response from ${operation}`,details:result.error.issues.slice(0,5).map(i=>i.path.join('.')).join(', '),retryable:false}); }
