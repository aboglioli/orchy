import { Link } from '@tanstack/react-router';

import { useAuthStore } from '../../state/auth-store';

export function OrgsPage() {
  const { user, memberships } = useAuthStore();

  return (
    <section className="orgs-page">
      <h2>Organizations</h2>

      {user?.is_platform_admin && (
        <div className="admin-notice">
          <p>You are a platform administrator. You can create and manage organizations.</p>
        </div>
      )}

      <h3>Your Organizations</h3>
      {memberships && memberships.length > 0 ? (
        <ul className="org-list">
          {memberships.map((membership) => (
            <li key={membership.id} className="org-item">
              <Link
                to="/orgs/$org"
                params={{ org: membership.org_id }}
                className="org-link"
              >
                <span className="org-id">{membership.org_id}</span>
                <span className={`org-role role-${membership.role}`}>
                  {membership.role}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      ) : (
        <p className="no-orgs">
          You are not a member of any organizations yet.
          {user?.is_platform_admin && ' Create an organization to get started.'}
        </p>
      )}
    </section>
  );
}
