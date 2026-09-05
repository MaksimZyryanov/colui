import { useState } from 'react';
import type { RegistryHealth, RegisterCandidateRequest } from '../../ipc/types';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import { useDiscovery, useDiscoveryMutations } from './hooks';
import { CandidateCard } from './CandidateCard';
import { OperationFeedback } from '../../ui/components/OperationFeedback';

type CandidateFeedback = { action: string; error: unknown; success: boolean };

export function DiscoverySection({ runtimeSessionId, inventoryGeneration, registryHealth }: { runtimeSessionId: string | null; inventoryGeneration: number; registryHealth: RegistryHealth | null }) {
  const discovery = useDiscovery(runtimeSessionId, inventoryGeneration, registryHealth);
  const mutations = useDiscoveryMutations();
  const candidates = discovery.data?.candidates ?? [];
  const [registering, setRegistering] = useState<ReadonlySet<string>>(new Set());
  const [ignoring, setIgnoring] = useState<ReadonlySet<string>>(new Set());
  const [candidateFeedback, setCandidateFeedback] = useState<ReadonlyMap<string, CandidateFeedback>>(new Map());
  const togglePending = mutations.configure.isPending || mutations.autoRegister.isPending;
  const enabled = discovery.data?.autoRegistrationEnabled ?? false;
  const recordFeedback = (key: string, feedback: CandidateFeedback) => setCandidateFeedback(current => new Map(current).set(key, feedback));
  const register = (request: RegisterCandidateRequest) => { if (registering.has(request.candidateId) || ignoring.has(request.candidateId)) return; const action = `Registration ${request.composeProjectName}`; setRegistering(current => new Set(current).add(request.candidateId)); void mutations.register.mutateAsync(request).then(() => recordFeedback(`register:${request.candidateId}`, { action, error: null, success: true }), error => recordFeedback(`register:${request.candidateId}`, { action, error, success: false })).finally(() => setRegistering(current => { const next = new Set(current); next.delete(request.candidateId); return next; })); };
  const ignore = (candidateId: string, composeProjectName: string) => { if (registering.has(candidateId) || ignoring.has(candidateId)) return; const action = `Ignore ${composeProjectName}`; setIgnoring(current => new Set(current).add(candidateId)); void mutations.ignore.mutateAsync({ candidateId }).then(() => recordFeedback(`ignore:${candidateId}`, { action, error: null, success: true }), error => recordFeedback(`ignore:${candidateId}`, { action, error, success: false })).finally(() => setIgnoring(current => { const next = new Set(current); next.delete(candidateId); return next; })); };
  return <section aria-labelledby="discovery-heading" className="feature-section">
    <header><div><p className="eyebrow">Runtime discovery</p><h2 id="discovery-heading">Discovered projects</h2></div><div className="auto-registration"><Button role="switch" aria-checked={enabled} aria-label="Automatic registration" disabled={!runtimeSessionId || togglePending} onClick={() => mutations.configure.mutate({ enabled: !enabled })}>{enabled ? 'Automatic: On' : 'Automatic: Off'}</Button>{runtimeSessionId ? <Button disabled={togglePending} onClick={() => mutations.autoRegister.mutate({ runtimeSessionId, inventoryGeneration })}>Register eligible now</Button> : null}</div></header>
    {[...candidateFeedback.entries()].map(([key, feedback]) => <OperationFeedback key={key} {...feedback} />)}<OperationFeedback action="Automatic registration configuration" error={mutations.configure.error} success={mutations.configure.isSuccess} /><OperationFeedback action="Automatic registration" error={mutations.autoRegister.error} success={mutations.autoRegister.isSuccess} />
    {!runtimeSessionId ? <p>Connect Docker to discover Compose projects.</p> : discovery.isLoading ? <div role="status" aria-label="Loading discovery">Loading discovery...</div> : discovery.isError ? <Alert variant="destructive">Unable to load discovery candidates</Alert> : candidates.length ? <div className="projects-grid">{candidates.map(candidate => <CandidateCard key={candidate.candidateId} candidate={candidate} pending={registering.has(candidate.candidateId) || ignoring.has(candidate.candidateId)} onRegister={() => register({ candidateId: candidate.candidateId, runtimeSessionId: candidate.runtimeSessionId, inventoryGeneration: candidate.inventoryGeneration, composeProjectName: candidate.composeProjectName, workingDirectory: candidate.workingDirectory, configFiles: candidate.configFiles })} onIgnore={() => ignore(candidate.candidateId, candidate.composeProjectName)} />)}</div> : <p>No unregistered Compose projects found.</p>}
  </section>;
}
