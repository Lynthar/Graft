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
import { statusLabel, stepLabel } from '../labels';

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

  // Candidate ids by save path, so a whole directory can be left out at once.
  const byDir = createMemo(() => {
    const dirs = new Map<string, number[]>();
    for (const c of preview()?.data.candidates ?? []) dirs.set(c.save_path, [...(dirs.get(c.save_path) ?? []), c.id]);
    return [...dirs.entries()].sort(([a], [b]) => a.localeCompare(b));
  });
  const risky = createMemo(() => new Set((preview()?.data.candidates ?? []).filter((c) => c.needs_confirmation).map((c) => c.id)));
  const chosenIn = (ids: number[]) => ids.filter((id) => selected().includes(id)).length;
  // Taking a directory back in skips the candidates that need their own confirmation.
  const toggleDir = (ids: number[]) =>
    setSelected(chosenIn(ids)
      ? selected().filter((id) => !ids.includes(id))
      : [...selected(), ...ids.filter((id) => !risky().has(id))]);

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
      <h1 class="page-title">辅种</h1>

      <div class="card bg-base-100 shadow-xl mb-6">
        <div class="card-body space-y-4">
          <label class="form-control max-w-md">
            <span class="label-text">从哪个下载器读取种子（qBittorrent）</span>
            <select class="select select-bordered" value={source()} onChange={(e) => setSource(e.currentTarget.value)}>
              <option value="">选择下载器</option>
              <For each={sourceClients()}>{(c) => <option value={c.id}>{c.name}</option>}</For>
            </select>
          </label>

          <div>
            <span class="label-text">去哪些站点查</span>
            <div class="flex flex-wrap gap-4 mt-2">
              <For each={usableSites()} fallback={<span class="text-sm">还没有启用的站点，请先到「站点」页设置。</span>}>
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
              预览
            </button>
            <Show when={running()}>
              <button class="btn btn-ghost" onClick={() => cancelTask(task()!.id)}>取消</button>
            </Show>
          </div>

          <Show when={running()}>
            <div class="text-sm">
              {task()!.progress.phase}
              <Show when={task()!.progress.total > 0}> — {task()!.progress.done} / {task()!.progress.total}</Show>
            </div>
            {/* Indeterminate means no value at all: assigning undefined to the property throws. */}
            <Show when={task()!.progress.total > 0} fallback={<progress class="progress progress-primary w-full" />}>
              <progress class="progress progress-primary w-full" value={task()!.progress.done} max={task()!.progress.total} />
            </Show>
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
              <h2 class="card-title">预览结果</h2>
              <div class="text-sm space-y-1">
                <div>
                  读到 {p().data.read.total} 个种子，其中 {p().data.read.incomplete} 个未完成（跳过）。
                  认出：{Object.entries(p().data.read.recognized).map(([k, v]) => `${k} ${v}`).join('、') || '无'}。
                </div>
                <For each={p().data.read.unrecognized}>{(r) => <div>未认出 {r.count} 个：{r.reason}</div>}</For>
                <For each={p().data.read.without_pieces}>{(r) => <div class="text-warning">未查询 {r.count} 个：{r.reason}</div>}</For>
                <For each={p().data.sites}>
                  {(s) => (
                    <div class={s.error ? 'text-error' : ''}>
                      {s.site_id}：查询 {s.queried} 个，命中 {s.found} 个
                      <Show when={s.already_seeding}>，其中已在做种 {s.already_seeding} 个</Show>
                      <Show when={s.error}>。{s.error}</Show>
                    </div>
                  )}
                </For>
              </div>

              <div class="flex gap-2">
                <button class="btn btn-xs" onClick={() => setSelected(p().data.candidates.filter((c) => !c.needs_confirmation).map((c) => c.id))}>
                  全选
                </button>
                <button class="btn btn-xs" onClick={() => setSelected([])}>全不选</button>
                <span class="text-sm self-center">已选 {selected().length} / {p().data.candidates.length}</span>
              </div>
              <Show when={byDir().length > 1}>
                <div class="text-sm">
                  <span class="mr-2">按保存目录：</span>
                  <For each={byDir()}>
                    {([dir, ids]) => (
                      <label class="label cursor-pointer inline-flex gap-2 mr-4">
                        <input type="checkbox" class="checkbox checkbox-sm" checked={chosenIn(ids) > 0}
                          onChange={() => toggleDir(ids)} />
                        <span class="label-text font-mono text-xs">{dir}</span>
                        <span class="label-text text-xs">（{chosenIn(ids)} / {ids.length}）</span>
                      </label>
                    )}
                  </For>
                </div>
              </Show>
              <div class="overflow-x-auto">
                <table class="table table-sm">
                  <thead>
                    <tr>
                      <th></th>
                      <th>种子</th>
                      <th>来源站</th>
                      <th>目标站</th>
                      <th>大小</th>
                      <th>保存路径</th>
                      <th>依据</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={p().data.candidates} fallback={<tr><td colspan="7">没有候选。</td></tr>}>
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
                            {c.evidence === 'pieces_equal' ? 'pieces 完全相同' : c.evidence}
                            <Show when={c.needs_confirmation}>
                              <div class="text-warning">需单独勾选确认：{c.note}</div>
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
                  <span class="label-text">加到</span>
                  <select class="select select-bordered" value={target()} onChange={(e) => setTarget(e.currentTarget.value)}>
                    <For each={clients() || []}>{(c) => <option value={c.id}>{c.name}</option>}</For>
                  </select>
                </label>
                <button class="btn btn-success" disabled={selected().length === 0 || !target() || running()} onClick={runExecute}>
                  加入选中的 {selected().length} 个
                </button>
              </div>
              <p class="text-xs text-base-content/70">
                种子以暂停状态加入，打上「graft」标签，保存路径沿用源种子的。下载器先校验已有数据，校验通过后由你自己开始做种。
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
                成功 {s().success}，跳过 {s().skipped}，失败 {s().failed}
                <Show when={s().not_attempted}>，未执行 {s().not_attempted}</Show>
              </h2>
              <table class="table table-sm">
                <tbody>
                  <For each={s().items}>
                    {(i) => (
                      <tr>
                        <td>
                          <span class={`badge badge-sm ${i.status === 'success' ? 'badge-success' : i.status === 'failed' ? 'badge-error' : 'badge-warning'}`}>
                            {statusLabel(i.status)}
                          </span>
                        </td>
                        <td class="max-w-xs truncate">{i.source_name}</td>
                        <td>{i.target_site}</td>
                        <td class="text-xs"><span class="badge badge-ghost badge-sm mr-1">{stepLabel(i.step)}</span>{i.message}</td>
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
