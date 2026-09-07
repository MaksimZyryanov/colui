import { useState } from 'react';
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
const unavailableStatus = (profileId: string): ProjectStatus => ({ profileId, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [] });
export function ProfileCard({ profile, initialStatus, runtimeReady, inventory }: { profile: ProfileSummary; initialStatus?: ProjectStatus; runtimeReady?: boolean; inventory?: QueryObserverResult<RuntimeInventory> }) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const runtime = useRuntimeState();
  const isRuntimeReady = runtimeReady ?? runtime.data?.state === 'ready';
  const status = useProjectStatus(profile, inventory, !initialStatus, Boolean(initialStatus) || isRuntimeReady);
  const details = useProfile(editing ? profile.id : undefined);
  const definitionError = initialStatus ? null : status.definitionError;
  const projectedStatus = initialStatus ?? status.data ?? (status.isError ? unavailableStatus(profile.id) : undefined);
  const label = projectedStatus ? projectStatusLabel(projectedStatus) : 'Loading status';
  const containers = inventory?.data?.projects.find(project => project.composeProjectName === profile.composeProjectName)?.containers ?? [];
  const sessionId = inventory?.data?.runtimeSessionId;
  return <ResourceRow name={profile.displayName} kind="registered"
    status={<span aria-label="Project status"><ResourceStatus label={label} tone={label === 'Running' ? 'running' : label === 'Stopped' ? 'stopped' : label === 'Invalid definition' || label === 'Partially running' ? 'warning' : 'unknown'} /></span>}
    actions={<>
      <Button iconOnly title="Edit project" onClick={() => setEditing(true)} aria-label={`Edit ${profile.displayName}`}>✎</Button>
      {projectedStatus ? <><Button iconOnly title="Status details" aria-label={`${open ? 'Hide' : 'Show'} status details`} onClick={() => setOpen(value => !value)} aria-expanded={open} aria-controls={`status-${profile.id}`}>ⓘ</Button>{definitionError && sessionId && containers.length ? <ContainerActions containers={containers} runtimeSessionId={sessionId} resourceName={profile.displayName} /> : null}<ActionMenu profileId={profile.id} revision={profile.revision} status={projectedStatus} runtimeReady={isRuntimeReady} showLifecycleActions={!definitionError} /></> : null}
    </>}
    footer={<>
      {status.isLoading && !initialStatus ? <span role="status" aria-label="Loading project status" className="skeleton">Loading status...</span> : status.isError && !status.data && !initialStatus ? <span>Status unavailable</span> : null}
      {open && projectedStatus ? <div id={`status-${profile.id}`} className="resource-details"><StatusDetails status={projectedStatus} /></div> : null}
      {editing ? details.isLoading ? <span role="status" aria-label="Loading project details" className="skeleton">Loading project details...</span> : details.data ? <ProfileFormDialog profile={profile} details={details.data} onClose={() => setEditing(false)} /> : details.isError ? <span role="alert">Unable to load project details</span> : null : null}
    </>}>
    <p className="resource-path">{profile.workingDirectory}</p>
    {containers.length ? containers.map(container => <ResourceRow key={`${sessionId}:${container.id}`} name={container.name} kind="container" status={<ResourceStatus label={container.state} tone={container.state} />} actions={sessionId ? <><ContainerActions containers={[container]} runtimeSessionId={sessionId} resourceName={container.name} /><ContainerLogsDialog containerId={container.id} containerName={container.name} runtimeSessionId={sessionId} compact /></> : null} />) : <p>{inventory?.data?.hasSnapshot ? 'No containers observed for this project.' : 'Container observations unavailable.'}</p>}
  </ResourceRow>;
}
