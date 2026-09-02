import type { ProfileSummary } from '../../../ipc/types';
import { ProfileCard } from './ProfileCard';
export function ProfileList({ profiles }: { profiles: ProfileSummary[] }) { return <div className="projects-grid">{profiles.map(profile => <ProfileCard key={profile.id} profile={profile} />)}</div>; }
