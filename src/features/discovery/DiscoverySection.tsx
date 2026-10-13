import { useState, type ReactNode } from 'react';
import type { DiscoveryCandidate, ProfileSummary, RegistryHealth, RegisterCandidateRequest, RuntimeInventory } from '../../ipc/types';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import { candidateResourceId, useDiscovery, useDiscoveryMutations, useRegistrationMetadata } from './hooks';
import { CandidateCard } from './CandidateCard';
import { OperationFeedback } from '../../ui/components/OperationFeedback';
import { ResourceList, type ResourceEntry } from '../projects/components/ResourceList';

type CandidateFeedback = { action: string; error: unknown; success: boolean };

export function DiscoverySection({ runtimeSessionId, inventoryGeneration, registryHealth, registeredProfiles, inventory, children }: { runtimeSessionId: string | null; inventoryGeneration: number; registryHealth: RegistryHealth | null; registeredProfiles?: ProfileSummary[]; inventory?: RuntimeInventory; children?: (resources: ResourceEntry[], state: { isLoading: boolean; isError: boolean; candidates: DiscoveryCandidate[] }) => ReactNode }) {
  const discovery = useDiscovery(runtimeSessionId, inventoryGeneration, registryHealth);
  useRegistrationMetadata(registeredProfiles, runtimeSessionId, discovery.data);
  const mutations = useDiscoveryMutations();
  const candidates = discovery.data?.candidates ?? [];
  const [registering, setRegistering] = useState<ReadonlySet<string>>(new Set());
  const [ignoring, setIgnoring] = useState<ReadonlySet<string>>(new Set());
  const [candidateFeedback, setCandidateFeedback] = useState<ReadonlyMap<string, CandidateFeedback>>(new Map());
  const togglePending = discovery.isPlaceholderData || mutations.configure.isPending || mutations.autoRegister.isPending;
  const enabled = discovery.data?.autoRegistrationEnabled ?? false;
  const recordFeedback = (key: string, feedback: CandidateFeedback) => setCandidateFeedback(current => new Map(current).set(key, feedback));
  const register = (request: RegisterCandidateRequest) => { if (registering.has(request.candidateId) || ignoring.has(request.candidateId)) return; const action = `Registration ${request.composeProjectName} (${request.candidateId})`; setRegistering(current => new Set(current).add(request.candidateId)); void mutations.register.mutateAsync(request).then(() => recordFeedback(`register:${request.candidateId}`, { action, error: null, success: true }), error => recordFeedback(`register:${request.candidateId}`, { action, error, success: false })).finally(() => setRegistering(current => { const next = new Set(current); next.delete(request.candidateId); return next; })); };
  const ignore = (candidateId: string, composeProjectName: string) => { if (registering.has(candidateId) || ignoring.has(candidateId)) return; const action = `Ignore ${composeProjectName} (${candidateId})`; setIgnoring(current => new Set(current).add(candidateId)); void mutations.ignore.mutateAsync({ candidateId }).then(() => recordFeedback(`ignore:${candidateId}`, { action, error: null, success: true }), error => recordFeedback(`ignore:${candidateId}`, { action, error, success: false })).finally(() => setIgnoring(current => { const next = new Set(current); next.delete(candidateId); return next; })); };
  const resources = candidates.filter(candidate => children || candidate.classification !== 'already_registered').map(candidate => {
    const group = inventory?.runtimeSessionId === candidate.runtimeSessionId ? inventory.composeObservationGroups.find(group => group.composeProjectName === candidate.composeProjectName && group.workingDirectory === candidate.workingDirectory && JSON.stringify(group.configFiles) === JSON.stringify(candidate.configFiles)) : undefined;
    const observedContainers = group ? inventory?.containers.filter(container => group.containerIds.includes(container.id)).sort((left, right) => left.name.localeCompare(right.name, undefined, { sensitivity: 'base', numeric: true }) || left.id.localeCompare(right.id)) : undefined;
    return {
    id: candidateResourceId(candidate.runtimeSessionId, candidate.candidateId),
    name: candidate.composeProjectName,
    content: <CandidateCard candidate={candidate} observedContainers={observedContainers} observationsCurrent={inventory?.freshness === 'fresh' && inventory?.runtimeSessionId === runtimeSessionId} pending={!runtimeSessionId || discovery.isPlaceholderData || registering.has(candidate.candidateId) || ignoring.has(candidate.candidateId)} onRegister={() => register({ candidateId: candidate.candidateId, runtimeSessionId: candidate.runtimeSessionId, inventoryGeneration: candidate.inventoryGeneration, composeProjectName: candidate.composeProjectName, workingDirectory: candidate.workingDirectory, configFiles: candidate.configFiles })} onIgnore={() => ignore(candidate.candidateId, candidate.composeProjectName)} />,
  }; });
  return <>
    <div className="resource-toolbar"><Button role="switch" aria-checked={enabled} aria-label="Automatic registration" disabled={!runtimeSessionId || togglePending} onClick={() => mutations.configure.mutate({ enabled: !enabled })}>{enabled ? 'Automatic: On' : 'Automatic: Off'}</Button>{runtimeSessionId ? <Button disabled={togglePending} onClick={() => mutations.autoRegister.mutate({ runtimeSessionId, inventoryGeneration })}>Register eligible now</Button> : null}</div>
    {[...candidateFeedback.entries()].map(([key, feedback]) => <div key={key} className={feedback.success ? 'sr-only' : undefined}><OperationFeedback {...feedback} /></div>)}<div className={mutations.configure.isSuccess ? 'sr-only' : undefined}><OperationFeedback action="Automatic registration configuration" error={mutations.configure.error} success={mutations.configure.isSuccess} /></div><div className={mutations.autoRegister.isSuccess ? 'sr-only' : undefined}><OperationFeedback action="Automatic registration" error={mutations.autoRegister.error} success={mutations.autoRegister.isSuccess} /></div>
    {!runtimeSessionId ? <p>Connect Docker to discover Compose projects.</p> : discovery.isLoading ? <div role="status" aria-label="Loading discovery">Loading discovery...</div> : discovery.isError ? <Alert variant="destructive">Unable to load discovery candidates</Alert> : null}
    {children ? children(resources, { ...discovery, candidates }) : <ResourceList resources={resources} empty={<p>{discovery.isError ? 'Discovery unavailable.' : discovery.isLoading ? 'Loading discovery…' : 'No unregistered Compose projects found.'}</p>} />}
  </>;
}
