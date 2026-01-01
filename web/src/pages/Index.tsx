import { Component, createSignal, createResource, For, Show } from 'solid-js';
import { fetchIndexStats, importFromClient, clearIndex, clearSiteIndex } from '../api/index';
import { fetchClients } from '../api/clients';

const Index: Component = () => {
  const [stats, { refetch: refetchStats }] = createResource(fetchIndexStats);
  const [clients] = createResource(fetchClients);
  const [importing, setImporting] = createSignal<string | null>(null);
  const [importResult, setImportResult] = createSignal<any>(null);

  const handleImport = async (clientId: string) => {
    setImporting(clientId);
    setImportResult(null);
    try {
      const result = await importFromClient(clientId);
      setImportResult(result);
      refetchStats();
    } catch (e) {
      setImportResult({ error: (e as Error).message });
    }
    setImporting(null);
  };

  const handleClearAll = async () => {
    if (confirm('Are you sure you want to clear all index entries?')) {
      await clearIndex();
      refetchStats();
    }
  };

  const handleClearSite = async (siteId: string) => {
    if (confirm(`Clear all index entries for ${siteId}?`)) {
      await clearSiteIndex(siteId);
      refetchStats();
    }
  };

  return (
    <div>
      <h1 class="page-title">Index Management</h1>

      {/* Import Section */}
      <div class="card bg-base-100 shadow-xl mb-6">
        <div class="card-body">
          <h2 class="card-title">Import from Download Client</h2>
          <p class="text-base-content/70 mb-4">
            Scan your download client for torrents and build a local index for cross-site matching.
          </p>

          <div class="flex flex-wrap gap-2">
            <For each={clients()}>
              {(client) => (
                <button
                  class="btn btn-outline"
                  onClick={() => handleImport(client.id)}
                  disabled={importing() !== null}
                >
                  {importing() === client.id ? (
                    <>
                      <span class="loading loading-spinner loading-sm"></span>
                      Importing...
                    </>
                  ) : (
                    <>Import from {client.name}</>
                  )}
                </button>
              )}
            </For>
          </div>

          <Show when={importResult()}>
            <div class={`alert ${importResult().error ? 'alert-error' : 'alert-success'} mt-4`}>
              <Show
                when={!importResult().error}
                fallback={<span>Error: {importResult().error}</span>}
              >
                <span>
                  Imported {importResult().imported} torrents
                  (skipped {importResult().skipped}, unrecognized {importResult().unrecognized})
                </span>
              </Show>
            </div>
          </Show>
        </div>
      </div>

      {/* Stats Section */}
      <div class="card bg-base-100 shadow-xl">
        <div class="card-body">
          <div class="flex justify-between items-center mb-4">
            <h2 class="card-title">Index Statistics</h2>
            <button class="btn btn-error btn-sm" onClick={handleClearAll}>
              Clear All
            </button>
          </div>

          <div class="stat bg-base-200 rounded-box mb-4">
            <div class="stat-title">Total Indexed</div>
            <div class="stat-value">{stats()?.total_entries || 0}</div>
            <div class="stat-desc">Torrents in index</div>
          </div>

          <div class="overflow-x-auto">
            <table class="table">
              <thead>
                <tr>
                  <th>Site</th>
                  <th>Count</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                <For each={stats()?.sites || []}>
                  {(site) => (
                    <tr>
                      <td class="font-medium">{site.site_id}</td>
                      <td>{site.count}</td>
                      <td>
                        <button
                          class="btn btn-xs btn-error btn-outline"
                          onClick={() => handleClearSite(site.site_id)}
                        >
                          Clear
                        </button>
                      </td>
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

export default Index;
