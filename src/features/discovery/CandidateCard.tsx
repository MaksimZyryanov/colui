import type { DiscoveryCandidate } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { ResourceRow, ResourceStatus } from '../projects/components/ResourceRow';

const labels = { new_unambiguous: 'New project', name_conflict: 'Name conflict', incomplete_metadata: 'Incomplete metadata', already_registered: 'Already registered' } as const;

export function CandidateCard({ candidate, pending, onRegister, onIgnore }: { candidate: DiscoveryCandidate; pending: boolean; onRegister: () => void; onIgnore: () => void }) {
  const eligible = candidate.classification === 'new_unambiguous' && !candidate.ignored;
  const warning = candidate.classification === 'name_conflict' || candidate.classification === 'incomplete_metadata';
  return <ResourceRow name={candidate.composeProjectName} kind="discovered"
    status={<ResourceStatus label={warning ? 'Registration unavailable' : candidate.classification === 'already_registered' ? 'Registered observation' : labels[candidate.classification]} tone={warning ? 'warning' : 'unknown'} />}
    notice={warning || candidate.ignored || candidate.classification === 'already_registered' ? <span className="resource-notice">{candidate.ignored ? 'Ignored' : labels[candidate.classification]}</span> : null}
    primaryAction={eligible ? <Button disabled={pending} onClick={onRegister} aria-label={`Register ${candidate.composeProjectName}`}>Register</Button> : null}
    actions={<Button disabled={pending || candidate.ignored} onClick={onIgnore} aria-label={`Ignore ${candidate.composeProjectName}`}>Ignore</Button>}>
    <p>{candidate.containerCount} container{candidate.containerCount === 1 ? '' : 's'}</p>
    {candidate.workingDirectory ? <code>{candidate.workingDirectory}</code> : <p>Working directory unavailable</p>}
    {candidate.conflicts.length ? <details><summary>Conflict evidence</summary>{candidate.conflicts.map((conflict, index) => <p key={`${conflict.source}-${conflict.profileId ?? index}`}>{conflict.source === 'registered_profile' ? 'Registered profile' : 'Runtime observation'}: {conflict.workingDirectory ?? 'unknown path'}</p>)}</details> : null}
  </ResourceRow>;
}
