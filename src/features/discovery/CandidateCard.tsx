import type { DiscoveryCandidate } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { Card } from '../../ui/components/Card';

const labels = { new_unambiguous: 'New project', name_conflict: 'Name conflict', incomplete_metadata: 'Incomplete metadata', already_registered: 'Already registered' } as const;

export function CandidateCard({ candidate, pending, onRegister, onIgnore }: { candidate: DiscoveryCandidate; pending: boolean; onRegister: () => void; onIgnore: () => void }) {
  const eligible = candidate.classification === 'new_unambiguous' && !candidate.ignored;
  return <Card aria-labelledby={`candidate-${candidate.candidateId}`}>
    <header><h3 id={`candidate-${candidate.candidateId}`}>{candidate.composeProjectName}</h3><span className={`status-chip status-${candidate.classification}`}>{labels[candidate.classification]}</span></header>
    <p>{candidate.containerCount} container{candidate.containerCount === 1 ? '' : 's'}</p>
    {candidate.workingDirectory ? <code>{candidate.workingDirectory}</code> : <p>Working directory unavailable</p>}
    {candidate.conflicts.length ? <details><summary>Conflict evidence</summary>{candidate.conflicts.map((conflict, index) => <p key={`${conflict.source}-${conflict.profileId ?? index}`}>{conflict.source === 'registered_profile' ? 'Registered profile' : 'Runtime observation'}: {conflict.workingDirectory ?? 'unknown path'}</p>)}</details> : null}
    <div className="card-actions">{eligible ? <Button disabled={pending} onClick={onRegister}>Register {candidate.composeProjectName}</Button> : null}<Button disabled={pending || candidate.ignored} onClick={onIgnore}>Ignore {candidate.composeProjectName}</Button></div>
  </Card>;
}
