import { useState } from 'react';
import type { ProfileSummary } from '../../../ipc/types';
import { Card } from '../../../ui/components/Card';
import { Button } from '../../../ui/components/Button';
import { useProjectStatus } from '../hooks/useProjectStatus';
import { StatusBadge } from './StatusBadge';
import { StatusDetails } from './StatusDetails';
export function ProfileCard({ profile }: { profile: ProfileSummary }) { const [open, setOpen] = useState(false); const status = useProjectStatus(profile.id); return <Card><h2>{profile.displayName}</h2><p>{profile.workingDirectory}</p>{status.isLoading ? <span>Loading status...</span> : status.isError ? <span>Status unavailable</span> : status.data ? <><StatusBadge status={status.data} /><Button onClick={() => setOpen(value => !value)} aria-expanded={open} aria-controls={`status-${profile.id}`}> {open ? 'Hide' : 'Show'} status details</Button>{open ? <div id={`status-${profile.id}`}><StatusDetails status={status.data} /></div> : null}</> : null}</Card>; }
