import { Component, createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { fetchClients } from '../api/clients';
import { fetchSites } from '../api/sites';
import {
  cancelTask,
  fetchTask,
  startExecute,
  startPreview,
  type ExecuteSummary,
  type Preview,
  type Task,
} from '../api/reseed';

const formatSize = (bytes: number) => {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit++;
  }
  return `${size.toFixed(1)} ${units[unit]}`;
};

const Reseed: Component = () => {
  const [clients] = createResource(fetchClients);
  const [sites] = createResource(fetchSites);

  const [source, setSource] = createSignal('');
  const [target, setTarget] = createSignal('');
  const [targetSites, setTargetSites] = createSignal<string[]>([]);
  const [task, setTask] = createSignal<Task<unknown> | null>(null);
  const [preview, setPreview] = createSignal<{ id: string; data: Preview } | null>(null);
  const [selected, setSelected] = createSignal<number[]>([]);
  const [summary, setSummary] = createSignal<ExecuteSummary | null>(null);
  const [error, setError] = createSignal('');

  const sourceClients = createMemo(() => (clients() || []).filter((c) => c.client_type === 'qbittorrent'));
  const usableSites = createMemo(() => (sites() || []).filter((s) => s.enabled));
  const running = () => task()?.status === 'running';

  const toggle = <T,>(list: T[], item: T) => (list.includes(item) ? list.filter((x) => x !== item) : [...list, item]);

  const follow = async <T,>(taskId: string): Promise<Task<T>> => {
    for (;;) {
      const t = await fetchTask<T>(taskId);
      setTask(t as Task<unknown>);
      if (t.status !== 'running') return t;
      await new Promise((r) => setTimeout(r, 1000));
    }
  };

  const runPreview = async () => {
    setError('');
    setPreview(null);
    setSummary(null);
    try {
      const { task_id } = await startPreview(source(), targetSites());
      const t = await follow<Preview>(task_id);
      if (t.result) {
        setPreview({ id: task_id, data: t.result });
        setSelected(t.result.candidates.filter((c) => !c.needs_confirmation).map((c) => c.id));
        if (!target()) setTarget(source());
      }
      if (t.error) setError(t.error);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const runExecute = async () => {
    const p = preview();
    if (!p) return;
    setError('');
    setSummary(null);
    const risky = p.data.candidates.filter((c) => c.needs_confirmation).map((c) => c.id);
    try {
      const { task_id } = await startExecute({
        preview_id: p.id,
        target_client_id: target(),
        candidate_ids: selected(),
        confirmed_risky_ids: selected().filter((id) => risky.includes(id)),
      });
      const t = await follow<ExecuteSummary>(task_id);
      if (t.result) setSummary(t.result);
      if (t.error) setError(t.error);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div>
      <h1 class="page-title">Reseed</h1>

      <div class="card bg-base-100 shadow-xl mb-6">
        <div class="card-body space-y-4">
          <label class="form-control max-w-md">
            <span class="label-text">Read torrents from (qBittorrent)</span>
            <select class="select select-bordered" value={source()} onChange={(e) => setSource(e.currentTarget.value)}>
              <option value="">Choose a client</option>
              <For each={sourceClients()}>{(c) => <option value={c.id}>{c.name}</option>}</For>
            </select>
          </label>

          <div>
            <span class="label-text">Look them up on</span>
            <div class="flex flex-wrap gap-4 mt-2">
              <For each={usableSites()} fallback={<span class="text-sm">No site is enabled yet — see Sites.</span>}>
                {(s) => (
                  <label class="label cursor-pointer gap-2">
                    <input type="checkbox" class="checkbox checkbox-sm" checked={targetSites().includes(s.id)}
                      onChange={() => setTargetSites(toggle(targetSites(), s.id))} />
                    <span class="label-text">{s.name}</span>
                  </label>
                )}
              </For>
            </div>
          </div>

          <div class="flex gap-2">
            <button class="btn btn-primary" disabled={!source() || targetSites().length === 0 || running()} onClick={runPreview}>
              Preview
            </button>
            <Show when={running()}>
              <button class="btn btn-ghost" onClick={() => cancelTask(task()!.id)}>Cancel</button>
            </Show>
          </div>

          <Show when={running()}>
            <div class="text-sm">
              {task()!.progress.phase}
              <Show when={task()!.progress.total > 0}> — {task()!.progress.done} / {task()!.progress.total}</Show>
            </div>
            <progress class="progress progress-primary w-full"
              value={task()!.progress.total ? task()!.progress.done : undefined} max={task()!.progress.total || 1} />
          </Show>
          <Show when={error()}>
            <div class="alert alert-error text-sm">{error()}</div>
          </Show>
        </div>
      </div>

      <Show when={preview()}>
        {(p) => (
          <div class="card bg-base-100 shadow-xl mb-6">
            <div class="card-body space-y-4">
              <h2 class="card-title">What was found</h2>
              <div class="text-sm space-y-1">
                <div>
                  {p().data.read.total} torrents read, {p().data.read.incomplete} not complete (skipped).
                  Recognised: {Object.entries(p().data.read.recognized).map(([k, v]) => `${k} ${v}`).join(', ') || 'none'}.
                </div>
                <For each={p().data.read.unrecognized}>{(r) => <div>Not recognised: {r.count} — {r.reason}</div>}</For>
                <For each={p().data.read.without_pieces}>{(r) => <div class="text-warning">Not looked up: {r.count} — {r.reason}</div>}</For>
                <For each={p().data.sites}>
                  {(s) => (
                    <div class={s.error ? 'text-error' : ''}>
                      {s.site_id}: asked about {s.queried}, found {s.found}
                      <Show when={s.already_seeding}>, already seeding {s.already_seeding}</Show>
                      <Show when={s.error}> — {s.error}</Show>
                    </div>
                  )}
                </For>
              </div>

              <div class="flex gap-2">
                <button class="btn btn-xs" onClick={() => setSelected(p().data.candidates.filter((c) => !c.needs_confirmation).map((c) => c.id))}>
                  Select all
                </button>
                <button class="btn btn-xs" onClick={() => setSelected([])}>Select none</button>
                <span class="text-sm self-center">{selected().length} of {p().data.candidates.length} selected</span>
              </div>
              <div class="overflow-x-auto">
                <table class="table table-sm">
                  <thead>
                    <tr>
                      <th></th>
                      <th>Torrent</th>
                      <th>From</th>
                      <th>To</th>
                      <th>Size</th>
                      <th>Save path</th>
                      <th>Evidence</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={p().data.candidates} fallback={<tr><td colspan="7">No candidates.</td></tr>}>
                      {(c) => (
                        <tr class={c.needs_confirmation ? 'bg-warning/10' : ''}>
                          <td>
                            <input type="checkbox" class="checkbox checkbox-sm" checked={selected().includes(c.id)}
                              onChange={() => setSelected(toggle(selected(), c.id))} />
                          </td>
                          <td class="max-w-xs truncate" title={c.source_name}>{c.source_name}</td>
                          <td>{c.source_site || '?'}</td>
                          <td>{c.target_site} #{c.target_torrent_id}</td>
                          <td>{formatSize(c.size)}</td>
                          <td class="text-xs">{c.save_path}</td>
                          <td class="text-xs" title={c.note}>
                            {c.evidence === 'pieces_equal' ? 'identical pieces' : c.evidence}
                            <Show when={c.needs_confirmation}>
                              <div class="text-warning">Check to confirm: {c.note}</div>
                            </Show>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>

              <div class="flex flex-wrap items-end gap-4">
                <label class="form-control">
                  <span class="label-text">Add to</span>
                  <select class="select select-bordered" value={target()} onChange={(e) => setTarget(e.currentTarget.value)}>
                    <For each={clients() || []}>{(c) => <option value={c.id}>{c.name}</option>}</For>
                  </select>
                </label>
                <button class="btn btn-success" disabled={selected().length === 0 || !target() || running()} onClick={runExecute}>
                  Add {selected().length} selected
                </button>
              </div>
              <p class="text-xs text-base-content/70">
                Torrents are added stopped, tagged "graft", at the source torrent's save path. The client checks the
                existing data before anything seeds; start them yourself once the check passes.
              </p>
            </div>
          </div>
        )}
      </Show>

      <Show when={summary()}>
        {(s) => (
          <div class="card bg-base-100 shadow-xl">
            <div class="card-body">
              <h2 class="card-title">
                Added {s().success}, skipped {s().skipped}, failed {s().failed}
                <Show when={s().not_attempted}>, not attempted {s().not_attempted}</Show>
              </h2>
              <table class="table table-sm">
                <tbody>
                  <For each={s().items}>
                    {(i) => (
                      <tr>
                        <td>
                          <span class={`badge badge-sm ${i.status === 'success' ? 'badge-success' : i.status === 'failed' ? 'badge-error' : 'badge-warning'}`}>
                            {i.status}
                          </span>
                        </td>
                        <td class="max-w-xs truncate">{i.source_name}</td>
                        <td>{i.target_site}</td>
                        <td class="text-xs"><span class="font-mono mr-1">{i.step}</span>{i.message}</td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
};

export default Reseed;
