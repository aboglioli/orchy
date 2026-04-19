type ProjectDashboardPageProps = {
  org: string;
  project: string;
};

export function ProjectDashboardPage({ org, project }: ProjectDashboardPageProps) {
  return (
    <section>
      <h2>Project dashboard</h2>
      <p>
        Dashboard placeholder for {org}/{project}
      </p>
    </section>
  );
}
