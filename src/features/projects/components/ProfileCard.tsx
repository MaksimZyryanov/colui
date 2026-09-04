import { useState } from 'react';
import type { ProfileSummary } from '../../../ipc/types';
import { Card } from '../../../ui/components/Card';
import { Button } from '../../../ui/components/Button';
import { useProjectStatus } from '../hooks/useProjectStatus';
import { StatusBadge } from './StatusBadge';
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
  const status = useProjectStatus(profile, inventory, !initialStatus);
  const runtime = useRuntimeState();
  const details = useProfile(editing ? profile.id : undefined);
  const projectedStatus = initialStatus ?? status.data ?? (status.isError ? unavailableStatus(profile.id) : undefined);
  return <Card><h2>{profile.displayName}</h2><p>{profile.workingDirectory}</p><Button onClick={() => setEditing(true)} aria-label={`Edit ${profile.displayName}`}>Edit</Button>{status.isLoading && !initialStatus ? <span role="status" aria-label="Loading project status" className="skeleton">Loading status...</span> : status.isError && !status.data && !initialStatus ? <span>Status unavailable</span> : null}{status.definitionError ? <span role="alert">Unable to load project definition</span> : null}{projectedStatus ? <><StatusBadge status={projectedStatus} /><Button onClick={() => setOpen(value => !value)} aria-expanded={open} aria-controls={`status-${profile.id}`}> {open ? 'Hide' : 'Show'} status details</Button>{open ? <div id={`status-${profile.id}`}><StatusDetails status={projectedStatus} /></div> : null}<ActionMenu profileId={profile.id} revision={profile.revision} status={projectedStatus} runtimeReady={runtimeReady ?? runtime.data?.state === 'ready'} /></> : null}{editing ? details.isLoading ? <span role="status" aria-label="Loading project details" className="skeleton">Loading project details...</span> : details.data ? <ProfileFormDialog profile={profile} details={details.data} onClose={() => setEditing(false)} /> : details.isError ? <span role="alert">Unable to load project details</span> : null : null}</Card>;
}
