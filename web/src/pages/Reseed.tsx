import { Component, createSignal, createResource, For, Show } from 'solid-js';
import { fetchClients } from '../api/clients';
import { fetchSites } from '../api/sites';
import { previewReseed, executeReseed, type PreviewResult } from '../api/reseed';

const Reseed: Component = () => {
  const [clients] = createResource(fetchClients);
  const [sites] = createResource(fetchSites);

  const [sourceClient, setSourceClient] = createSignal('');
  const [targetClient, setTargetClient] = createSignal('');
  const [selectedSites, setSelectedSites] = createSignal<string[]>([]);
  const [addPaused, setAddPaused] = createSignal(false);
  const [skipChecking, setSkipChecking] = createSignal(false);

  const [preview, setPreview] = createSignal<PreviewResult | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [executing, setExecuting] = createSignal(false);
  const [result, setResult] = createSignal<any>(null);

  const toggleSite = (siteId: string) => {
    const current = selectedSites();
    if (current.includes(siteId)) {
      setSelectedSites(current.filter(id => id !== siteId));
    } else {
      setSelectedSites([...current, siteId]);
    }
  };

  const handlePreview = async () => {
    if (!sourceClient() || selectedSites().length === 0) return;

    setLoading(true);
    setPreview(null);
    setResult(null);

    try {
      const result = await previewReseed({
        source_client_id: sourceClient(),
        target_site_ids: selectedSites(),
      });
      setPreview(result);
    } catch (e) {
      console.error(e);
    }

    setLoading(false);
  };

  const handleExecute = async () => {
    if (!sourceClient() || !targetClient() || selectedSites().length === 0) return;

    setExecuting(true);
    setResult(null);

    try {
      const execResult = await executeReseed({
        source_client_id: sourceClient(),
        target_client_id: targetClient(),
        target_site_ids: selectedSites(),
        add_paused: addPaused(),
        skip_checking: skipChecking(),
      });
      setResult(execResult);
      setPreview(null);
    } catch (e) {
      console.error(e);
    }

    setExecuting(false);
  };

  const formatSize = (bytes: number) => {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = bytes;
    let unitIndex = 0;
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    return `${size.toFixed(2)} ${units[unitIndex]}`;
  };

  return (
    <div>
      <h1 class="page-title">Reseed</h1>

      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Configuration */}
        <div class="card bg-base-100 shadow-xl">
          <div class="card-body">
            <h2 class="card-title">Configuration</h2>

            <div class="form-control mb-4">
              <label class="label">
                <span class="label-text">Source Client</span>
              </label>
              <select
                class="select select-bordered"
                value={sourceClient()}
                onChange={(e) => setSourceClient(e.currentTarget.value)}
              >
                <option value="">Select client...</option>
                <For each={clients()}>
                  {(client) => (
                    <option value={client.id}>{client.name}</option>
                  )}
                </For>
              </select>
            </div>

            <div class="form-control mb-4">
              <label class="label">
                <span class="label-text">Target Client</span>
              </label>
              <select
                class="select select-bordered"
                value={targetClient()}
                onChange={(e) => setTargetClient(e.currentTarget.value)}
              >
                <option value="">Select client...</option>
                <For each={clients()}>
                  {(client) => (
                    <option value={client.id}>{client.name}</option>
                  )}
                </For>
              </select>
            </div>

            <div class="form-control mb-4">
              <label class="label">
                <span class="label-text">Target Sites</span>
              </label>
              <div class="flex flex-wrap gap-2">
                <For each={sites()?.filter(s => s.enabled && s.has_passkey)}>
                  {(site) => (
                    <button
                      class={`btn btn-sm ${selectedSites().includes(site.id) ? 'btn-primary' : 'btn-outline'}`}
                      onClick={() => toggleSite(site.id)}
                    >
                      {site.name}
                    </button>
                  )}
                </For>
              </div>
            </div>

            <div class="divider"></div>

            <div class="form-control">
              <label class="label cursor-pointer">
                <span class="label-text">Add paused</span>
                <input
                  type="checkbox"
                  class="checkbox"
                  checked={addPaused()}
                  onChange={(e) => setAddPaused(e.currentTarget.checked)}
                />
              </label>
            </div>

            <div class="form-control mb-4">
              <label class="label cursor-pointer">
                <span class="label-text">Skip hash checking</span>
                <input
                  type="checkbox"
                  class="checkbox"
                  checked={skipChecking()}
                  onChange={(e) => setSkipChecking(e.currentTarget.checked)}
                />
              </label>
            </div>

            <div class="flex gap-2">
              <button
                class="btn btn-outline flex-1"
                onClick={handlePreview}
                disabled={loading() || !sourceClient() || selectedSites().length === 0}
              >
                {loading() ? <span class="loading loading-spinner"></span> : 'Preview'}
              </button>
              <button
                class="btn btn-primary flex-1"
                onClick={handleExecute}
                disabled={executing() || !sourceClient() || !targetClient() || selectedSites().length === 0}
              >
                {executing() ? <span class="loading loading-spinner"></span> : 'Execute'}
              </button>
            </div>
          </div>
        </div>

        {/* Results */}
        <div class="card bg-base-100 shadow-xl">
          <div class="card-body">
            <h2 class="card-title">Results</h2>

            <Show when={result()}>
              <div class="alert alert-success mb-4">
                <span>
                  Completed: {result().success} success, {result().failed} failed, {result().skipped} skipped
                </span>
              </div>
            </Show>

            <Show when={preview()}>
              <div class="stat bg-base-200 rounded-box mb-4">
                <div class="stat-title">Matches Found</div>
                <div class="stat-value">{preview()?.matches.length}</div>
                <div class="stat-desc">Total: {formatSize(preview()?.total_size || 0)}</div>
              </div>

              <div class="overflow-x-auto max-h-96">
                <table class="table table-xs">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Target</th>
                      <th>Confidence</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={preview()?.matches.slice(0, 50)}>
                      {(match) => (
                        <tr>
                          <td class="max-w-xs truncate" title={match.source_name}>
                            {match.source_name}
                          </td>
                          <td>{match.target_site}</td>
                          <td>
                            <span class={`badge badge-sm ${
                              match.confidence >= 0.9 ? 'badge-success' :
                              match.confidence >= 0.7 ? 'badge-warning' : 'badge-error'
                            }`}>
                              {(match.confidence * 100).toFixed(0)}%
                            </span>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>

            <Show when={!preview() && !result()}>
              <div class="text-center text-base-content/50 py-8">
                Click "Preview" to see matching torrents
              </div>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
};

export default Reseed;
