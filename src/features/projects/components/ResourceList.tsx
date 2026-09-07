import { useState, type ReactNode } from 'react';

export type ResourceEntry = { id: string; name: string; searchNames?: string[]; content: ReactNode };

export function ResourceList({ resources, empty }: { resources: ResourceEntry[]; empty?: ReactNode }) {
  const [search, setSearch] = useState('');
  const query = search.trim().toLocaleLowerCase();
  const sorted = [...resources].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true }) || a.id.localeCompare(b.id));
  const matches = (resource: ResourceEntry) => [resource.name, ...resource.searchNames ?? []].some(name => name.toLocaleLowerCase().includes(query));
  return <div className="resources">
    <label className="resource-search">Search resources<input className="ui-input" type="search" value={search} onChange={event => setSearch(event.target.value)} placeholder="Project or container name…" /></label>
    <ul className="resource-list" aria-label="Resources">
      {sorted.map(resource => <li key={resource.id} hidden={!matches(resource)}>{resource.content}</li>)}
      {!sorted.some(matches) ? <li className="resource-empty">{query ? <p role="status">No matching resources</p> : empty}</li> : null}
    </ul>
  </div>;
}
