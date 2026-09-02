import type { ReactNode } from 'react';
import { ProjectsView } from '../features/projects/ProjectsView';

export function ProjectsRoute(): ReactNode {
  return <ProjectsView />;
}

export function AppRoutes(): ReactNode {
  return <ProjectsRoute />;
}
