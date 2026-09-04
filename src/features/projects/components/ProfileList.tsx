import type { ProfileSummary } from '../../../ipc/types';
import { ProfileCard } from './ProfileCard';
import type { QueryObserverResult } from '@tanstack/react-query';
import type { RuntimeInventory } from '../../../ipc/types';
export function ProfileList({ profiles, inventory }: { profiles: ProfileSummary[]; inventory: QueryObserverResult<RuntimeInventory> }) { return <div className="projects-grid">{profiles.map(profile => <ProfileCard key={profile.id} profile={profile} inventory={inventory} />)}</div>; }
