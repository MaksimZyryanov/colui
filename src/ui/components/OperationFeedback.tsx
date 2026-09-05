import { AppErrorException } from '../../ipc/errors';

export function OperationFeedback({ action, error, success }: { action: string; error: unknown; success: boolean }) {
  if (error) {
    const typed = error instanceof AppErrorException ? error : null;
    const message = error instanceof Error ? error.message : 'Operation failed';
    const details = typed?.details;
    return <div role="alert" aria-label={`${action} failed`} className="ui-alert ui-alert-destructive"><strong>{typed?.code ?? 'operation_failed'}</strong>: {message}{details ? <details><summary>Technical details</summary><p>{details}</p></details> : null}</div>;
  }
  return success ? <p role="status" aria-label={`${action} succeeded`} className="operation-status">{action} succeeded.</p> : null;
}
