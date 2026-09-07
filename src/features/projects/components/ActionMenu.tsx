import * as RadixAlertDialog from '@radix-ui/react-alert-dialog';
import { useRef, useState, type RefObject } from 'react';
import type { ProjectStatus } from '../../../ipc/types';
import { AppErrorException } from '../../../ipc/errors';
import { Button } from '../../../ui/components/Button';
import { DropdownMenu } from '../../../ui/components/DropdownMenu';
import { useLifecycleActions } from '../hooks/useLifecycleActions';
import { useProfileMutations } from '../hooks/useProfileMutations';

type Props = { profileId: string; revision: number; status: ProjectStatus; runtimeReady: boolean };
type Confirmation = 'tear-down' | 'remove';

export function ActionMenu({ profileId, revision, status, runtimeReady }: Props) {
  const lifecycle = useLifecycleActions();
  const profile = useProfileMutations();
  const [tearDownOpen, setTearDownOpen] = useState(false);
  const [removeOpen, setRemoveOpen] = useState(false);
  const [error, setError] = useState<AppErrorException | Error | null>(null);
  const moreActionsRef = useRef<HTMLButtonElement>(null);
  const tearDownCancelRef = useRef<HTMLButtonElement>(null);
  const removeCancelRef = useRef<HTMLButtonElement>(null);
  const pending = lifecycle.apply.isPending || lifecycle.stop.isPending || lifecycle.restart.isPending || lifecycle.tearDown.isPending || profile.remove.isPending;
  const lifecycleDisabled = !runtimeReady || status.runtime.presence !== 'present' || status.definition.state === 'unchecked' || status.definition.state === 'invalid' || Boolean(status.operation && !['succeeded', 'failed'].includes(status.operation.phase)) || pending;
  const run = async (action: 'apply' | 'stop' | 'restart') => { setError(null); try { await lifecycle[action].mutateAsync(profileId); } catch (caught) { setError(caught instanceof Error ? caught : new Error('Unable to perform project action')); } };
  const confirm = async (kind: Confirmation) => { setError(null); try { if (kind === 'remove') { await profile.remove.mutateAsync({ profileId, expectedRevision: revision }); setRemoveOpen(false); } else { await lifecycle.tearDown.mutateAsync(profileId); setTearDownOpen(false); } } catch (caught) { setError(caught instanceof Error ? caught : new Error('Unable to perform project action')); } };
  const dialog = (kind: Confirmation, open: boolean, setOpen: (value: boolean) => void, cancelRef: RefObject<HTMLButtonElement>) => { const remove = kind === 'remove'; return <RadixAlertDialog.Root open={open} onOpenChange={value => { if (!value && !pending) { setOpen(false); setError(null); } }}><RadixAlertDialog.Portal><RadixAlertDialog.Overlay className="ui-overlay" /><RadixAlertDialog.Content className="ui-dialog" onOpenAutoFocus={event => { event.preventDefault(); cancelRef.current?.focus(); }} onCloseAutoFocus={event => { event.preventDefault(); moreActionsRef.current?.focus(); }}><RadixAlertDialog.Title>{remove ? 'Remove profile' : 'Tear down project'}</RadixAlertDialog.Title><RadixAlertDialog.Description>{remove ? 'Remove this profile. Docker is untouched.' : 'Tear down this project. Containers and networks are removed, but profile remains.'}</RadixAlertDialog.Description>{error ? <div role="alert">{error.message}{error instanceof AppErrorException && error.details ? `: ${error.details}` : ''}</div> : null}<div className="ui-dialog-actions"><RadixAlertDialog.Cancel asChild><Button ref={cancelRef} disabled={pending}>Cancel</Button></RadixAlertDialog.Cancel><RadixAlertDialog.Action asChild><Button destructive disabled={pending} onClick={event => { event.preventDefault(); void confirm(kind); }}>{remove ? 'Remove profile' : 'Tear down'}</Button></RadixAlertDialog.Action></div></RadixAlertDialog.Content></RadixAlertDialog.Portal></RadixAlertDialog.Root>; };
  return <><div className="project-actions"><Button iconOnly aria-label="Stop" title="Stop" disabled={lifecycleDisabled} onClick={() => void run('stop')}>□</Button><Button iconOnly aria-label="Restart" title="Restart" disabled={lifecycleDisabled} onClick={() => void run('restart')}>↻</Button><DropdownMenu trigger={<Button iconOnly ref={moreActionsRef} aria-label="More actions" title="More actions">⋯</Button>} items={[{ label: 'Apply', onSelect: () => void run('apply'), disabled: lifecycleDisabled }, { label: 'Tear down', onSelect: () => { setError(null); setTearDownOpen(true); }, destructive: true, disabled: lifecycleDisabled }, { label: 'Remove profile', onSelect: () => { setError(null); setRemoveOpen(true); }, destructive: true, disabled: pending }]} /></div>{dialog('tear-down', tearDownOpen, setTearDownOpen, tearDownCancelRef)}{dialog('remove', removeOpen, setRemoveOpen, removeCancelRef)}</>;
}
