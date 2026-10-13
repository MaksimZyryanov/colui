import { useRef, useState } from 'react';
import type { ProfileSummary } from '../../../ipc/types';
import { Button } from '../../../ui/components/Button';
import { useProjectStatus } from '../hooks/useProjectStatus';
import { projectStatusLabel } from './StatusBadge';
import { ResourceRow, ResourceStatus } from './ResourceRow';
import { ContainerLogsDialog } from '../../containers/ContainerLogsDialog';
import { ContainerActions } from '../../containers/ContainerActions';
import { StatusDetails } from './StatusDetails';
import { useProfile } from '../hooks/useProfile';
import { ProfileFormDialog } from './ProfileFormDialog';
import { ActionMenu } from './ActionMenu';
import { useRuntimeState } from '../../../features/runtime/hooks/useRuntimeSession';
import type { ProjectStatus } from '../../../ipc/types';
import type { QueryObserverResult } from '@tanstack/react-query';
import type { RuntimeInventory } from '../../../ipc/types';
import { Icon } from '../../../ui/components/Icon';
import { AppErrorException } from '../../../ipc/errors';
import { useContainerActionsPending } from '../../containers/hooks';
const unavailableStatus = (profileId: string): ProjectStatus => ({ profileId, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [] });
export function ProfileCard({ profile, initialStatus, runtimeReady, inventory }: { profile: ProfileSummary; initialStatus?: ProjectStatus; runtimeReady?: boolean; inventory?: QueryObserverResult<RuntimeInventory> }) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const runtime = useRuntimeState();
  const runtimeConnected = runtimeReady ?? runtime.data?.state === 'ready';
  const status = useProjectStatus(profile, inventory, !initialStatus, Boolean(initialStatus) || runtimeConnected);
  const containerOperation = useContainerActionsPending(status.containers.map(container => container.id), inventory?.data?.runtimeSessionId ?? '');
  const observationReady = runtimeReady ?? status.observationCurrent;
  const isRuntimeReady = observationReady && !containerOperation;
  const details = useProfile(editing ? profile.id : undefined);
  const projectedStatus = initialStatus ?? status.data ?? (status.isError ? unavailableStatus(profile.id) : undefined);
  const label = !initialStatus && status.isLoading ? 'Loading status' : projectedStatus ? projectStatusLabel(projectedStatus) : 'Loading status';
  const containers = status.containers;
  const sessionId = inventory?.data?.runtimeSessionId;
  const definitionFailure = status.definitionError instanceof AppErrorException ? status.definitionError.code : (status.definitionError as { code?: string } | null)?.code;
  const previousMode = useRef({ revision: profile.revision, fallback: false });
  if (previousMode.current.revision !== profile.revision) previousMode.current = { revision: profile.revision, fallback: false };
  if (definitionFailure !== 'operation_conflict') previousMode.current.fallback = projectedStatus?.definition.state === 'invalid' || definitionFailure === 'definition_failed';
  const containerFallback = previousMode.current.fallback;
  return <ResourceRow name={profile.displayName} kind="registered"
    status={<span aria-label="Project status"><ResourceStatus label={label} tone={label === 'Running' ? 'running' : label === 'Stopped' ? 'stopped' : label === 'Invalid definition' || label === 'Partially running' ? 'warning' : 'unknown'} /></span>}
    actions={<>
      <Button iconOnly title="Edit project" onClick={() => setEditing(true)} aria-label={`Edit ${profile.displayName}`}><Icon name="settings-2" /></Button>
      <Button iconOnly title="Status details" disabled={!projectedStatus} aria-label={`${open ? 'Hide' : 'Show'} status details`} onClick={() => setOpen(value => !value)} aria-expanded={open} aria-controls={`status-${profile.id}`}><Icon name="activity" /></Button>{containerFallback && sessionId && containers.length ? <ContainerActions containers={containers} runtimeSessionId={sessionId} resourceName={profile.displayName} disabled={!isRuntimeReady} /> : null}<ActionMenu profileId={profile.id} revision={profile.revision} status={projectedStatus ?? unavailableStatus(profile.id)} runtimeReady={isRuntimeReady} showLifecycleActions={!containerFallback} />
    </>}
    footer={<>
      {open && projectedStatus ? <div id={`status-${profile.id}`} className="resource-details"><StatusDetails status={projectedStatus} /></div> : null}
      {editing ? details.isLoading ? <span role="status" aria-label="Loading project details" className="skeleton">Loading project details...</span> : details.data ? <ProfileFormDialog profile={profile} details={details.data} onClose={() => setEditing(false)} /> : details.isError ? <span role="alert">Unable to load project details</span> : null : null}
    </>}>
    <p className="resource-path">{profile.workingDirectory}</p>
    {containers.length ? containers.map(container => <ResourceRow key={`${sessionId}:${container.id}`} name={container.name} kind="container" status={<ResourceStatus label={observationReady ? container.state : `Last observed: ${container.state}`} tone={observationReady ? container.state : 'unknown'} />} actions={sessionId ? <><ContainerActions containers={[container]} runtimeSessionId={sessionId} resourceName={container.name} disabled={!observationReady || Boolean(projectedStatus?.operation)} /><ContainerLogsDialog containerId={container.id} containerName={container.name} runtimeSessionId={sessionId} compact disabled={!observationReady} /></> : null} />) : <p>{status.isLoading ? 'Loading container observations…' : projectedStatus?.runtime.presence === 'unavailable' ? 'Container association unavailable.' : inventory?.data?.hasSnapshot ? 'No containers observed for this project.' : 'Container observations unavailable.'}</p>}
  </ResourceRow>;
}
