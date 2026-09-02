import * as RadixAlertDialog from '@radix-ui/react-alert-dialog';
import { useState } from 'react';
import type { ProjectStatus } from '../../../ipc/types';
import { AppErrorException } from '../../../ipc/errors';
import { Button } from '../../../ui/components/Button';
import { DropdownMenu } from '../../../ui/components/DropdownMenu';
import { useLifecycleActions } from '../hooks/useLifecycleActions';
import { useProfileMutations } from '../hooks/useProfileMutations';

type Props = { profileId: string; revision: number; status: ProjectStatus; runtimeReady: boolean };
type Confirmation = 'tear-down' | 'remove' | null;

export function ActionMenu({ profileId, revision, status, runtimeReady }: Props) {
  const lifecycle = useLifecycleActions();
  const profile = useProfileMutations();
  const [confirmation, setConfirmation] = useState<Confirmation>(null);
  const [error, setError] = useState<AppErrorException | Error | null>(null);
  const pending = lifecycle.apply.isPending || lifecycle.stop.isPending || lifecycle.restart.isPending || lifecycle.tearDown.isPending || profile.remove.isPending;
  const runtimeAvailable = runtimeReady && status.runtime.presence === 'present';
  const semanticAvailable = status.definition.state !== 'unchecked' && status.definition.state !== 'invalid';
  const operationActive = Boolean(status.operation && !['succeeded', 'failed'].includes(status.operation.phase));
  const disabled = !runtimeAvailable || !semanticAvailable || operationActive || pending;
  const lifecycleDisabled = !runtimeAvailable || !semanticAvailable || operationActive;
  const run = async (action: 'apply' | 'stop' | 'restart') => {
    setError(null);
    try { await lifecycle[action].mutateAsync(profileId); } catch (caught) { setError(caught instanceof Error ? caught : new Error('Unable to perform project action')); }
  };
  const confirm = async () => {
    if (!confirmation) return;
    setError(null);
    try {
      if (confirmation === 'remove') await profile.remove.mutateAsync({ profileId, expectedRevision: revision });
      else await lifecycle.tearDown.mutateAsync(profileId);
      setConfirmation(null);
    } catch (caught) { setError(caught instanceof Error ? caught : new Error('Unable to perform project action')); }
  };
  const title = confirmation === 'remove' ? 'Remove profile' : 'Tear down project';
  const confirmLabel = confirmation === 'remove' ? 'Remove profile' : 'Tear down';
  const description = confirmation === 'remove' ? 'Remove this profile. Docker is untouched.' : 'Tear down this project. Containers and networks are removed, but profile remains.';
  return <>
    <div className="ui-dialog-actions">
      <Button disabled={lifecycleDisabled || lifecycle.stop.isPending} onClick={() => void run('stop')}>Stop</Button>
      <Button disabled={lifecycleDisabled || lifecycle.restart.isPending} onClick={() => void run('restart')}>Restart</Button>
      <DropdownMenu trigger={<Button aria-label="More actions">More</Button>} items={[
        { label: 'Apply', onSelect: () => void run('apply'), destructive: false, disabled: lifecycleDisabled || lifecycle.apply.isPending },
        { label: 'Tear down', onSelect: () => setConfirmation('tear-down'), destructive: true, disabled: lifecycleDisabled || lifecycle.tearDown.isPending },
        { label: 'Remove profile', onSelect: () => setConfirmation('remove'), destructive: true, disabled: lifecycleDisabled || profile.remove.isPending },
      ]} />
    </div>
    {confirmation ? <RadixAlertDialog.Root open onOpenChange={open => { if (!open && !pending) { setConfirmation(null); setError(null); } }}>
      <RadixAlertDialog.Portal><RadixAlertDialog.Overlay className="ui-overlay" /><RadixAlertDialog.Content className="ui-dialog">
        <RadixAlertDialog.Title>{title}</RadixAlertDialog.Title><RadixAlertDialog.Description>{description}</RadixAlertDialog.Description>
        {error ? <div role="alert">{error.message}{error instanceof AppErrorException && error.details ? `: ${error.details}` : ''}</div> : null}
        <div className="ui-dialog-actions"><RadixAlertDialog.Cancel asChild><Button disabled={pending}>Cancel</Button></RadixAlertDialog.Cancel><RadixAlertDialog.Action asChild><Button destructive disabled={pending} onClick={event => { event.preventDefault(); void confirm(); }}>{confirmLabel}</Button></RadixAlertDialog.Action></div>
      </RadixAlertDialog.Content></RadixAlertDialog.Portal>
    </RadixAlertDialog.Root> : null}
  </>;
}
