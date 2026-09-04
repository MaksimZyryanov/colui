import { useState } from 'react';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import { ErrorBoundary } from '../../ui/components/ErrorBoundary';
import { useProfiles } from './hooks/useProfiles';
import { EmptyState } from './components/EmptyState';
import { ProfileFormDialog } from './components/ProfileFormDialog';
import { ProfileList } from './components/ProfileList';
import { AppErrorException } from '../../ipc/errors';
import { useInventory } from '../runtime/hooks/useInventory';
export function ProjectsView() { const [adding, setAdding] = useState(false); const profiles = useProfiles(); const inventory = useInventory(); const error = profiles.error instanceof AppErrorException ? profiles.error : profiles.error instanceof Error ? profiles.error : null; return <ErrorBoundary><main><header><h1>Projects</h1>{profiles.data?.length ? <Button onClick={() => setAdding(true)}>Add Project</Button> : null}</header>{profiles.isLoading ? <div role="status" aria-label="Loading projects" className="skeleton">Loading projects...</div> : profiles.isError ? <Alert variant="destructive">{error?.message ?? 'Unable to load projects'}{error instanceof AppErrorException && error.details ? `: ${error.details}` : null}</Alert> : profiles.data?.length ? <ProfileList profiles={profiles.data} inventory={inventory} /> : <EmptyState onAdd={() => setAdding(true)} />}{adding ? <ProfileFormDialog onClose={() => setAdding(false)} /> : null}</main></ErrorBoundary>; }
