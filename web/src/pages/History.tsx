import { Component, createResource, For } from 'solid-js';
import { fetchHistory, type HistoryEntry } from '../api/reseed';

const History: Component = () => {
  const [history] = createResource(() => fetchHistory({ limit: 100 }));

  return (
    <div>
      <h1 class="page-title">Reseed History</h1>

      <div class="table-container">
        <table class="table">
          <thead>
            <tr>
              <th>Time</th>
              <th>Source</th>
              <th>Target</th>
              <th>Status</th>
              <th>Message</th>
            </tr>
          </thead>
          <tbody>
            <For each={history()}>
              {(entry) => (
                <tr>
                  <td class="text-sm">{new Date(entry.created_at).toLocaleString()}</td>
                  <td>
                    <code class="text-xs">{entry.info_hash.substring(0, 8)}...</code>
                    {entry.source_site && (
                      <span class="badge badge-ghost badge-sm ml-2">{entry.source_site}</span>
                    )}
                  </td>
                  <td>
                    <span class="badge badge-outline badge-sm">{entry.target_site}</span>
                  </td>
                  <td>
                    <span class={`badge ${
                      entry.status === 'success' ? 'badge-success' :
                      entry.status === 'failed' ? 'badge-error' : 'badge-warning'
                    }`}>
                      {entry.status}
                    </span>
                  </td>
                  <td class="text-sm text-base-content/70 max-w-xs truncate">
                    {entry.message || '-'}
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default History;
