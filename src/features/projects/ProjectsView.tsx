import { useState } from 'react';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import { ErrorBoundary } from '../../ui/components/ErrorBoundary';
import { useProfiles } from './hooks/useProfiles';
import { ProfileFormDialog } from './components/ProfileFormDialog';
import { ProfileCard } from './components/ProfileCard';
import { ResourceList, type ResourceEntry } from './components/ResourceList';
import { AppErrorException } from '../../ipc/errors';
import { useInventory } from '../runtime/hooks/useInventory';
import { useRuntimeState } from '../runtime/hooks/useRuntimeSession';
import { useDiagnostics } from '../diagnostics/hooks';
import { DiscoverySection } from '../discovery/DiscoverySection';
import { ContainerCard } from '../containers/ContainerCard';

export function ProjectsView() {
  const [adding, setAdding] = useState(false);
  const profiles = useProfiles();
  const inventory = useInventory({ owner: false });
  const runtime = useRuntimeState();
  const diagnostics = useDiagnostics({ owner: false });
  const error = profiles.error instanceof Error ? profiles.error : null;
  const sessionId = runtime.data?.state === 'ready' ? runtime.data.context.sessionId : inventory.data?.runtimeSessionId ?? null;
  const resources: ResourceEntry[] = (profiles.data ?? []).map(profile => ({
    id: `profile:${profile.id}`,
    name: profile.displayName,
    searchNames: [profile.composeProjectName, ...inventory.data?.projects.find(project => project.composeProjectName === profile.composeProjectName)?.containers.map(container => container.name) ?? []],
    content: <ProfileCard profile={profile} inventory={inventory} />,
  }));
  const observedSession = inventory.data?.runtimeSessionId;
  if (observedSession) {
    resources.push(...(inventory.data?.standaloneContainers ?? []).map(container => ({
      id: `container:${observedSession}:${container.id}`,
      name: container.name,
      content: <ContainerCard container={container} runtimeSessionId={observedSession} />,
    })));
  }
  return <ErrorBoundary><main className="page">
    <header className="page-heading"><div><p className="eyebrow">Local environments</p><h1>Projects</h1><p>Compose projects and containers, together.</p></div><Button onClick={() => setAdding(true)}>Add Project</Button></header>
    {profiles.isLoading ? <div role="status" aria-label="Loading projects" className="skeleton">Loading projects...</div> : null}
    {profiles.isError ? <Alert variant="destructive">{error?.message ?? 'Unable to load projects'}{error instanceof AppErrorException && error.details ? `: ${error.details}` : null}</Alert> : null}
    {inventory.isError ? <Alert variant="destructive">Unable to refresh container observations</Alert> : null}
    {inventory.data?.freshness === 'stale' ? <p role="status">Container observations are stale. Showing the last snapshot.</p> : !inventory.data?.hasSnapshot ? <p>Container observations unavailable.</p> : null}
    <DiscoverySection runtimeSessionId={sessionId} inventoryGeneration={inventory.data?.generation ?? 0} registryHealth={diagnostics.data?.registry.health ?? null}>
      {(discovered, discovery) => <ResourceList resources={[...resources, ...discovered]} empty={profiles.isError || inventory.isError || discovery.isError ? <p>Resources could not be fully loaded.</p> : profiles.isLoading || discovery.isLoading ? <p>Loading resources…</p> : <><h2>No projects yet</h2><p>Add a profile or connect Docker to observe resources.</p></>} />}
    </DiscoverySection>
    {adding ? <ProfileFormDialog onClose={() => setAdding(false)} /> : null}
  </main></ErrorBoundary>;
}
