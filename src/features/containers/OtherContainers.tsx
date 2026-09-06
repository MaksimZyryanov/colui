import type { RuntimeInventory } from '../../ipc/types';
import { ContainerCard } from './ContainerCard';

export function OtherContainers({ inventory }: { inventory: RuntimeInventory | undefined }) {
  const containers = inventory?.standaloneContainers ?? [];
  return <section aria-labelledby="other-containers-heading" className="feature-section"><header><div><p className="eyebrow">Docker inventory</p><h2 id="other-containers-heading">Other containers</h2></div></header>{!inventory?.runtimeSessionId ? <p>Connect Docker to inspect standalone containers.</p> : containers.length ? <div className="projects-grid">{containers.map(container => <ContainerCard key={container.id} container={container} runtimeSessionId={inventory.runtimeSessionId!} />)}</div> : <p>No standalone containers found.</p>}</section>;
}
