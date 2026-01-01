import { Component, createResource, For } from 'solid-js';
import { fetchStats } from '../api/stats';

const Dashboard: Component = () => {
  const [stats] = createResource(fetchStats);

  return (
    <div>
      <h1 class="page-title">Dashboard</h1>

      {/* Stats Cards */}
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
        <div class="stat bg-base-100 shadow rounded-box">
          <div class="stat-figure text-primary">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" viewBox="0 0 20 20" fill="currentColor">
              <path d="M7 3a1 1 0 000 2h6a1 1 0 100-2H7zM4 7a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1zM2 11a2 2 0 012-2h12a2 2 0 012 2v4a2 2 0 01-2 2H4a2 2 0 01-2-2v-4z" />
            </svg>
          </div>
          <div class="stat-title">Total Index</div>
          <div class="stat-value text-primary">{stats()?.index?.total_entries || 0}</div>
          <div class="stat-desc">Torrents indexed</div>
        </div>

        <div class="stat bg-base-100 shadow rounded-box">
          <div class="stat-figure text-secondary">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M3 5a2 2 0 012-2h10a2 2 0 012 2v8a2 2 0 01-2 2h-2.22l.123.489.804.804A1 1 0 0113 18H7a1 1 0 01-.707-1.707l.804-.804L7.22 15H5a2 2 0 01-2-2V5zm5.771 7H5V5h10v7H8.771z" clip-rule="evenodd" />
            </svg>
          </div>
          <div class="stat-title">Clients</div>
          <div class="stat-value text-secondary">{stats()?.clients || 0}</div>
          <div class="stat-desc">Download clients</div>
        </div>

        <div class="stat bg-base-100 shadow rounded-box">
          <div class="stat-figure text-success">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
            </svg>
          </div>
          <div class="stat-title">Today Success</div>
          <div class="stat-value text-success">{stats()?.today?.success || 0}</div>
          <div class="stat-desc">Reseeded today</div>
        </div>

        <div class="stat bg-base-100 shadow rounded-box">
          <div class="stat-figure text-error">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" viewBox="0 0 20 20" fill="currentColor">
              <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd" />
            </svg>
          </div>
          <div class="stat-title">Today Failed</div>
          <div class="stat-value text-error">{stats()?.today?.failed || 0}</div>
          <div class="stat-desc">Failed today</div>
        </div>
      </div>

      {/* Index by Site */}
      <div class="card bg-base-100 shadow-xl">
        <div class="card-body">
          <h2 class="card-title">Index by Site</h2>
          <div class="overflow-x-auto">
            <table class="table">
              <thead>
                <tr>
                  <th>Site</th>
                  <th>Count</th>
                </tr>
              </thead>
              <tbody>
                <For each={stats()?.index?.sites || []}>
                  {(site) => (
                    <tr>
                      <td>{site.site_id}</td>
                      <td>{site.count}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
};

export default Dashboard;
