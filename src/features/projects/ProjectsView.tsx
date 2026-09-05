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
import { useRuntimeState } from '../runtime/hooks/useRuntimeSession';
import { useDiagnostics } from '../diagnostics/hooks';
import { DiscoverySection } from '../discovery/DiscoverySection';
import { OtherContainers } from '../containers/OtherContainers';
export function ProjectsView() { const [adding, setAdding] = useState(false); const profiles = useProfiles(); const inventory = useInventory({ owner: false }); const runtime = useRuntimeState(); const diagnostics = useDiagnostics({ owner: false }); const error = profiles.error instanceof AppErrorException ? profiles.error : profiles.error instanceof Error ? profiles.error : null; const sessionId = runtime.data?.state === 'ready' ? runtime.data.context.sessionId : inventory.data?.runtimeSessionId ?? null; return <ErrorBoundary><main className="page"><header className="page-heading"><div><p className="eyebrow">Local environments</p><h1>Projects</h1><p>Registered projects and workloads observed from Docker.</p></div>{profiles.data?.length ? <Button onClick={() => setAdding(true)}>Add Project</Button> : null}</header><section aria-labelledby="registered-projects-heading" className="feature-section"><h2 id="registered-projects-heading">Registered projects</h2>{profiles.isLoading ? <div role="status" aria-label="Loading projects" className="skeleton">Loading projects...</div> : profiles.isError ? <Alert variant="destructive">{error?.message ?? 'Unable to load projects'}{error instanceof AppErrorException && error.details ? `: ${error.details}` : null}</Alert> : profiles.data?.length ? <ProfileList profiles={profiles.data} inventory={inventory} /> : <EmptyState onAdd={() => setAdding(true)} />}</section><DiscoverySection runtimeSessionId={sessionId} inventoryGeneration={inventory.data?.generation ?? 0} registryHealth={diagnostics.data?.registry.health ?? null} /><OtherContainers inventory={inventory.data} />{adding ? <ProfileFormDialog onClose={() => setAdding(false)} /> : null}</main></ErrorBoundary>; }
