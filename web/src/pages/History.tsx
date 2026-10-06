import { Component, createResource, For } from 'solid-js';
import { fetchHistory } from '../api/reseed';

const History: Component = () => {
  const [history] = createResource(() => fetchHistory(100));

  return (
    <div>
      <h1 class="page-title">Reseed History</h1>

      <div class="table-container">
        <table class="table">
          <thead>
            <tr>
              <th>Time</th>
              <th>Torrent</th>
              <th>Target</th>
              <th>Status</th>
              <th>Detail</th>
            </tr>
          </thead>
          <tbody>
            <For each={history()}>
              {(entry) => (
                <tr>
                  <td class="text-sm">{new Date(entry.created_at.replace(' ', 'T') + 'Z').toLocaleString()}</td>
                  <td class="max-w-xs truncate" title={entry.source_name}>
                    {entry.source_name}
                    {entry.source_site && (
                      <span class="badge badge-ghost badge-sm ml-2">{entry.source_site}</span>
                    )}
                  </td>
                  <td>
                    <span class="badge badge-outline badge-sm">{entry.target_site}</span>
                    <span class="text-xs ml-1">#{entry.target_torrent_id}</span>
                  </td>
                  <td>
                    <span class={`badge ${
                      entry.status === 'success' ? 'badge-success' :
                      entry.status === 'failed' ? 'badge-error' : 'badge-warning'
                    }`}>
                      {entry.status}
                    </span>
                  </td>
                  <td class="text-sm text-base-content/70 max-w-md" title={entry.message}>
                    <span class="font-mono text-xs mr-1">{entry.step}</span>
                    {entry.message}
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
