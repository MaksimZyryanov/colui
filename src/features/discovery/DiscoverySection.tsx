import type { RegistryHealth } from '../../ipc/types';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import { useDiscovery, useDiscoveryMutations } from './hooks';
import { CandidateCard } from './CandidateCard';

export function DiscoverySection({ runtimeSessionId, inventoryGeneration, registryHealth }: { runtimeSessionId: string | null; inventoryGeneration: number; registryHealth: RegistryHealth | null }) {
  const discovery = useDiscovery(runtimeSessionId, inventoryGeneration, registryHealth);
  const mutations = useDiscoveryMutations();
  const candidates = discovery.data?.candidates ?? [];
  const pendingCandidate = mutations.register.isPending ? mutations.register.variables?.candidateId : mutations.ignore.isPending ? mutations.ignore.variables?.candidateId : undefined;
  const togglePending = mutations.configure.isPending || mutations.autoRegister.isPending;
  const enabled = discovery.data?.autoRegistrationEnabled ?? false;
  return <section aria-labelledby="discovery-heading" className="feature-section">
    <header><div><p className="eyebrow">Runtime discovery</p><h2 id="discovery-heading">Discovered projects</h2></div><div className="auto-registration"><Button role="switch" aria-checked={enabled} aria-label="Automatic registration" disabled={!runtimeSessionId || togglePending} onClick={() => mutations.configure.mutate({ enabled: !enabled })}>{enabled ? 'Automatic: On' : 'Automatic: Off'}</Button>{runtimeSessionId ? <Button disabled={togglePending} onClick={() => mutations.autoRegister.mutate({ runtimeSessionId, inventoryGeneration })}>Register eligible now</Button> : null}</div></header>
    {!runtimeSessionId ? <p>Connect Docker to discover Compose projects.</p> : discovery.isLoading ? <div role="status" aria-label="Loading discovery">Loading discovery...</div> : discovery.isError ? <Alert variant="destructive">Unable to load discovery candidates</Alert> : candidates.length ? <div className="projects-grid">{candidates.map(candidate => <CandidateCard key={candidate.candidateId} candidate={candidate} pending={pendingCandidate === candidate.candidateId} onRegister={() => mutations.register.mutate({ candidateId: candidate.candidateId, runtimeSessionId: candidate.runtimeSessionId, inventoryGeneration: candidate.inventoryGeneration, composeProjectName: candidate.composeProjectName, workingDirectory: candidate.workingDirectory, configFiles: candidate.configFiles })} onIgnore={() => mutations.ignore.mutate({ candidateId: candidate.candidateId })} />)}</div> : <p>No unregistered Compose projects found.</p>}
  </section>;
}
