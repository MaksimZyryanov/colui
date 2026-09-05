import type { ReactNode } from 'react';
import { ProjectsView } from '../features/projects/ProjectsView';
import { DiagnosticsView } from '../features/diagnostics/DiagnosticsView';

export function ProjectsRoute(): ReactNode {
  return <ProjectsView />;
}

export function DiagnosticsRoute(): ReactNode { return <DiagnosticsView />; }

export function AppRoutes(): ReactNode {
  return <ProjectsRoute />;
}
