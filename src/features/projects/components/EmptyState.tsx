import { Button } from '../../../ui/components/Button';
export function EmptyState({ onAdd }: { onAdd: () => void }) { return <section><h2>No projects yet</h2><p>Add profile to begin.</p><Button onClick={onAdd}>Add Project</Button></section>; }
