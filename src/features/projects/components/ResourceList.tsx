import { useEffect, useState, type ReactNode } from 'react';
import { ResourceExpansionContext } from './ResourceRow';

export type ResourceEntry = { id: string; previousId?: string; name: string; searchNames?: string[]; content: ReactNode };

export function ResourceList({ resources, empty }: { resources: ResourceEntry[]; empty?: ReactNode }) {
  const [search, setSearch] = useState('');
  const [expansions, setExpansions] = useState<ReadonlyMap<string, boolean>>(new Map());
  useEffect(() => {
    setExpansions(current => {
      let next: Map<string, boolean> | undefined;
      for (const resource of resources) if (resource.previousId && !current.has(resource.id) && current.has(resource.previousId)) {
        next ??= new Map(current);
        next.set(resource.id, current.get(resource.previousId)!);
      }
      return next ?? current;
    });
  }, [resources]);
  const query = search.trim().toLocaleLowerCase();
  const sorted = [...resources].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true }) || a.id.localeCompare(b.id));
  const matches = (resource: ResourceEntry) => [resource.name, ...resource.searchNames ?? []].some(name => name.toLocaleLowerCase().includes(query));
  return <div className="resources">
    <label className="resource-search">Search resources<input className="ui-input" type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder="Project or container name…" /></label>
    <ul className="resource-list" aria-label="Resources">
      {sorted.map(resource => <li key={resource.id} hidden={!matches(resource)}><ResourceExpansionContext.Provider value={{ expanded: expansions.get(resource.id) ?? (resource.previousId ? expansions.get(resource.previousId) : undefined) ?? false, toggle: () => setExpansions(current => new Map(current).set(resource.id, !(current.get(resource.id) ?? (resource.previousId ? current.get(resource.previousId) : undefined) ?? false))) }}>{resource.content}</ResourceExpansionContext.Provider></li>)}
      {!sorted.some(matches) ? <li className="resource-empty">{query ? <p role="status">No matching resources</p> : empty}</li> : null}
    </ul>
  </div>;
}
