import { useEffect, useState } from 'react';
import { DiagnosticsView } from '../features/diagnostics/DiagnosticsView';
import { useDiagnostics } from '../features/diagnostics/hooks';
import { ProjectsView } from '../features/projects/ProjectsView';
import { useInventory } from '../features/runtime/hooks/useInventory';

type Route = 'projects' | 'diagnostics';
function Navigation({ route, onNavigate, mobile = false }: { route: Route; onNavigate: (route: Route) => void; mobile?: boolean }) {
  return <nav aria-label={mobile ? 'Mobile primary' : 'Primary'} className={mobile ? 'mobile-nav' : 'side-nav'}><a href="#projects" aria-current={route === 'projects' ? 'page' : undefined} onClick={event => { event.preventDefault(); onNavigate('projects'); }}>Projects</a><a href="#diagnostics" aria-current={route === 'diagnostics' ? 'page' : undefined} onClick={event => { event.preventDefault(); onNavigate('diagnostics'); }}>Diagnostics</a></nav>;
}
export function AppShell() {
  const initial = location.hash === '#diagnostics' ? 'diagnostics' : 'projects';
  const [route, setRoute] = useState<Route>(initial);
  useInventory();
  useDiagnostics();
  useEffect(() => {
    const routeChanged = () => setRoute(location.hash === '#diagnostics' ? 'diagnostics' : 'projects');
    window.addEventListener('hashchange', routeChanged);
    return () => window.removeEventListener('hashchange', routeChanged);
  }, []);
  const navigate = (next: Route) => { location.hash = next; setRoute(next); };
  return <div className="app-shell"><aside className="sidebar"><div className="brand"><span>Co</span>LUI</div><Navigation route={route} onNavigate={navigate} /></aside><div className="app-content">{route === 'projects' ? <ProjectsView /> : <DiagnosticsView />}</div><Navigation mobile route={route} onNavigate={navigate} /></div>;
}
