import {
  Navigate,
  Outlet,
  RouterProvider,
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';

import { EventsPage } from '../features/events/events-page';
import { AgentsPage } from '../features/agents/agents-page';
import { LoginPage } from '../features/auth/login-page';
import { KnowledgePage } from '../features/knowledge/knowledge-page';
import { AppLayout } from '../features/layout/app-layout';
import { LocksPage } from '../features/locks/locks-page';
import { MessagesPage } from '../features/messages/messages-page';
import { OrgsPage } from '../features/orgs/orgs-page';
import { ProjectDashboardPage } from '../features/projects/project-dashboard-page';
import { TaskDetailPage } from '../features/tasks/task-detail-page';
import { TasksPage } from '../features/tasks/tasks-page';
import { useAuthStore } from '../state/auth-store';

export type AppRoute = {
  path: string;
};

const projectRouteSuffixes = [
  'dashboard',
  'tasks',
  'tasks/:id',
  'knowledge',
  'agents',
  'messages',
  'locks',
  'events',
] as const;

const projectRoutePrefix = '/orgs/:org/projects/:project';

export const projectScopedRoutes = projectRouteSuffixes.map((suffix) => ({
  path: `${projectRoutePrefix}/${suffix}`,
})) as ReadonlyArray<AppRoute>;

export const appRoutes: ReadonlyArray<AppRoute> = [
  { path: '/login' },
  { path: '/orgs' },
  { path: '/orgs/:org' },
  ...projectScopedRoutes,
];

function ProtectedRoute() {
  const { isAuthenticated } = useAuthStore();

  if (!isAuthenticated) {
    return <Navigate to="/login" />;
  }

  return (
    <AppLayout>
      <Outlet />
    </AppLayout>
  );
}

function PublicRoute() {
  const { isAuthenticated } = useAuthStore();

  if (isAuthenticated) {
    return <Navigate to="/orgs" />;
  }

  return (
    <AppLayout>
      <Outlet />
    </AppLayout>
  );
}

const rootRoute = createRootRoute({
  component: Outlet,
});

const publicRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'public',
  component: PublicRoute,
});

const protectedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'protected',
  component: ProtectedRoute,
});

const loginRoute = createRoute({
  getParentRoute: () => publicRoute,
  path: '/login',
  component: LoginPage,
});

const orgsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs',
  component: OrgsPage,
});

const orgDetailsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org',
  component: () => {
    const { org } = orgDetailsRoute.useParams();
    return <section>Organization details for {org}</section>;
  },
});

const projectDashboardRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/dashboard',
  component: () => {
    const { org, project } = projectDashboardRoute.useParams();
    return <ProjectDashboardPage org={org} project={project} />;
  },
});

const tasksRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/tasks',
  component: () => {
    const { org, project } = tasksRoute.useParams();
    return <TasksPage org={org} project={project} />;
  },
});

const taskDetailsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/tasks/$id',
  component: () => {
    const { org, project, id } = taskDetailsRoute.useParams();
    return <TaskDetailPage org={org} project={project} taskId={id} />;
  },
});

const knowledgeRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/knowledge',
  component: () => {
    const { org, project } = knowledgeRoute.useParams();
    return <KnowledgePage org={org} project={project} />;
  },
});

const agentsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/agents',
  component: () => {
    const { org } = agentsRoute.useParams();
    return <AgentsPage org={org} />;
  },
});

const messagesRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/messages',
  component: () => {
    const { org, project } = messagesRoute.useParams();
    return <MessagesPage org={org} project={project} />;
  },
});

const locksRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/locks',
  component: () => {
    const { org, project } = locksRoute.useParams();
    return <LocksPage org={org} project={project} />;
  },
});

const eventsRoute = createRoute({
  getParentRoute: () => protectedRoute,
  path: '/orgs/$org/projects/$project/events',
  component: () => {
    const { org, project } = eventsRoute.useParams();
    return <EventsPage org={org} project={project} />;
  },
});

const routeTree = rootRoute.addChildren([
  publicRoute.addChildren([loginRoute]),
  protectedRoute.addChildren([
    orgsRoute,
    orgDetailsRoute,
    projectDashboardRoute,
    tasksRoute,
    taskDetailsRoute,
    knowledgeRoute,
    agentsRoute,
    messagesRoute,
    locksRoute,
    eventsRoute,
  ]),
]);

export const appRouter = createRouter({ routeTree });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof appRouter;
  }
}

export function AppRouterProvider() {
  return <RouterProvider router={appRouter} />;
}
