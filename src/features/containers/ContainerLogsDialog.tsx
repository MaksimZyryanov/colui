import { useEffect, useState } from 'react';
import { Dialog } from '../../ui/components/Dialog';
import { Button } from '../../ui/components/Button';
import { useContainerLogs } from '../diagnostics/hooks';
import { Icon } from '../../ui/components/Icon';

export function ContainerLogsDialog({ containerId, containerName, runtimeSessionId, compact = false }: { containerId: string; containerName: string; runtimeSessionId: string; compact?: boolean }) {
  const logs = useContainerLogs({ containerId, runtimeSessionId });
  const [open, setOpen] = useState(false);
  useEffect(() => { if (open) void logs.refetch(); }, [open]);
  return <Dialog trigger={compact ? <Button iconOnly aria-label={`View logs for ${containerName}`} title="View logs"><Icon name="logs" /></Button> : <Button>View logs for {containerName}</Button>} title={`Logs: ${containerName}`} description="Newest retained container output. Logs do not update automatically." open={open} onOpenChange={setOpen}>
    {logs.isFetching ? <div role="status" aria-label="Loading container logs">Loading logs...</div> : logs.isError ? <p role="alert">Container logs are no longer available. Refresh inventory and try again.</p> : logs.data ? <>{logs.data.truncated ? <p role="status" aria-label="Logs truncated">Earlier output was truncated. Showing newest {logs.data.retainedBytes.toLocaleString()} bytes.</p> : null}<pre className="container-logs" tabIndex={0}>{logs.data.text}</pre></> : null}
  </Dialog>;
}
