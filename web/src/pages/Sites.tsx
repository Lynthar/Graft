import { Component, createResource, createSignal, For, Show } from 'solid-js';
import { createSite, deleteSite, fetchSites, updateSite, type Site } from '../api/sites';

interface FormState {
  id: string;
  name: string;
  base_url: string;
  template_type: string;
  download_pattern: string;
  domains: string;
  passkey: string;
  cookie: string;
  authkey: string;
  rate_limit_rpm: number;
  daily_limit: number;
  enabled: boolean;
}

const emptyForm = (): FormState => ({
  id: '',
  name: '',
  base_url: 'https://',
  template_type: 'nexusphp',
  download_pattern: '',
  domains: '',
  passkey: '',
  cookie: '',
  authkey: '',
  rate_limit_rpm: 10,
  daily_limit: 20,
  enabled: true,
});

const Sites: Component = () => {
  const [sites, { refetch }] = createResource(fetchSites);
  // null: closed; 'new': adding a custom site; otherwise the site being edited
  const [editing, setEditing] = createSignal<Site | 'new' | null>(null);
  const [form, setForm] = createSignal<FormState>(emptyForm());
  const [error, setError] = createSignal('');

  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => setForm({ ...form(), [key]: value });

  const openNew = () => {
    setForm(emptyForm());
    setError('');
    setEditing('new');
  };

  const openEdit = (site: Site) => {
    setForm({
      ...emptyForm(),
      id: site.id,
      name: site.name,
      base_url: site.base_url,
      template_type: site.template_type,
      download_pattern: site.download_pattern,
      domains: site.domains.join(', '),
      rate_limit_rpm: site.rate_limit_rpm,
      daily_limit: site.daily_limit,
      enabled: site.enabled,
    });
    setError('');
    setEditing(site);
  };

  const domainList = () => form().domains.split(/[,\s]+/).map((d) => d.trim()).filter(Boolean);

  const save = async (e: Event) => {
    e.preventDefault();
    const f = form();
    const current = editing();
    try {
      if (current === 'new') {
        await createSite({
          id: f.id,
          name: f.name,
          base_url: f.base_url,
          template_type: f.template_type,
          download_pattern: f.download_pattern || undefined,
          domains: domainList().length ? domainList() : undefined,
          passkey: f.passkey || undefined,
          cookie: f.cookie || undefined,
          authkey: f.authkey || undefined,
          rate_limit_rpm: f.rate_limit_rpm,
          daily_limit: f.daily_limit,
          enabled: f.enabled,
        });
      } else if (current) {
        // Credentials left blank keep their stored value.
        await updateSite(current.id, {
          name: f.name,
          base_url: f.base_url,
          download_pattern: f.download_pattern,
          domains: domainList(),
          passkey: f.passkey || undefined,
          cookie: f.cookie || undefined,
          authkey: f.authkey || undefined,
          rate_limit_rpm: f.rate_limit_rpm,
          daily_limit: f.daily_limit,
          enabled: f.enabled,
        });
      }
      setEditing(null);
      refetch();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const toggle = async (site: Site) => {
    try {
      await updateSite(site.id, { enabled: !site.enabled });
      refetch();
    } catch (err) {
      alert((err as Error).message);
    }
  };

  const remove = async (site: Site) => {
    if (!confirm(`确定删除 ${site.name}？`)) return;
    try {
      await deleteSite(site.id);
      refetch();
    } catch (err) {
      alert((err as Error).message);
    }
  };

  const isNew = () => editing() === 'new';

  return (
    <div>
      <div class="flex justify-between items-center mb-6">
        <h1 class="page-title mb-0">站点</h1>
        <button class="btn btn-primary" onClick={openNew}>添加自定义站点</button>
      </div>

      <p class="mb-4 text-base-content/70">
        内置站点默认停用：编辑填好 passkey 后再启用。只有 NexusPHP 站点能按内容直查；凭据以明文存在数据库文件里。
      </p>

      <div class="table-container">
        <table class="table">
          <thead>
            <tr>
              <th>站点</th>
              <th>类型</th>
              <th>Tracker 域名</th>
              <th>Passkey</th>
              <th>限额</th>
              <th>启用</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <For each={sites()}>
              {(site) => (
                <tr>
                  <td>
                    <div class="font-medium">{site.name}</div>
                    <div class="text-xs text-base-content/60">{site.base_url}</div>
                  </td>
                  <td><span class="badge badge-outline">{site.template_type}</span></td>
                  <td class="text-xs">{site.domains.join(', ')}</td>
                  <td>{site.has_passkey ? '已填' : '—'}</td>
                  <td class="text-xs">每分钟 {site.rate_limit_rpm} 次 · 每天 {site.daily_limit} 个</td>
                  <td>
                    <input type="checkbox" class="toggle toggle-success toggle-sm" checked={site.enabled}
                      onChange={() => toggle(site)} />
                  </td>
                  <td class="whitespace-nowrap">
                    <button class="btn btn-sm btn-ghost" onClick={() => openEdit(site)}>编辑</button>
                    <Show when={!site.builtin}>
                      <button class="btn btn-sm btn-error btn-outline" onClick={() => remove(site)}>删除</button>
                    </Show>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      <Show when={editing()}>
        <div class="modal modal-open">
          <div class="modal-box max-w-xl">
            <h3 class="font-bold text-lg mb-4">{isNew() ? '添加自定义站点' : `编辑 ${form().name}`}</h3>
            <form onSubmit={save} class="space-y-3">
              <Show when={isNew()}>
                <div class="grid grid-cols-2 gap-3">
                  <label class="form-control">
                    <span class="label-text">id（a–z、0–9、-、_）</span>
                    <input class="input input-bordered" value={form().id} onInput={(e) => set('id', e.currentTarget.value)} required />
                  </label>
                  <label class="form-control">
                    <span class="label-text">类型</span>
                    <select class="select select-bordered" value={form().template_type}
                      onChange={(e) => set('template_type', e.currentTarget.value)}>
                      <option value="nexusphp">nexusphp</option>
                      <option value="unit3d">unit3d</option>
                      <option value="gazelle">gazelle</option>
                    </select>
                  </label>
                </div>
              </Show>
              <label class="form-control">
                <span class="label-text">名称</span>
                <input class="input input-bordered" value={form().name} onInput={(e) => set('name', e.currentTarget.value)} required />
              </label>
              <label class="form-control">
                <span class="label-text">地址（只收 https）</span>
                <input class="input input-bordered" value={form().base_url} onInput={(e) => set('base_url', e.currentTarget.value)} required />
              </label>
              <label class="form-control">
                <span class="label-text">Tracker 域名（逗号分隔，子域名也算）</span>
                <input class="input input-bordered" value={form().domains} placeholder="不填则用地址的域名"
                  onInput={(e) => set('domains', e.currentTarget.value)} />
              </label>
              <label class="form-control">
                <span class="label-text">下载路径（可用 {'{id}'}、{'{passkey}'}、{'{authkey}'}）</span>
                <input class="input input-bordered font-mono text-sm" value={form().download_pattern}
                  placeholder="不填则用该类型的缺省值" onInput={(e) => set('download_pattern', e.currentTarget.value)} />
              </label>
              <label class="form-control">
                <span class="label-text">Passkey{isNew() ? '' : '（留空则保留已存的）'}</span>
                <input type="password" class="input input-bordered" value={form().passkey}
                  onInput={(e) => set('passkey', e.currentTarget.value)} autocomplete="off" />
              </label>
              <Show when={form().template_type === 'gazelle'}>
                <label class="form-control">
                  <span class="label-text">Authkey</span>
                  <input type="password" class="input input-bordered" value={form().authkey}
                    onInput={(e) => set('authkey', e.currentTarget.value)} autocomplete="off" />
                </label>
              </Show>
              <div class="grid grid-cols-2 gap-3">
                <label class="form-control">
                  <span class="label-text">每分钟请求数（1–60）</span>
                  <input type="number" min="1" max="60" class="input input-bordered" value={form().rate_limit_rpm}
                    onInput={(e) => set('rate_limit_rpm', Number(e.currentTarget.value))} />
                </label>
                <label class="form-control">
                  <span class="label-text">每天下载数</span>
                  <input type="number" min="0" max="1000" class="input input-bordered" value={form().daily_limit}
                    onInput={(e) => set('daily_limit', Number(e.currentTarget.value))} />
                </label>
              </div>
              <label class="label cursor-pointer justify-start gap-3">
                <input type="checkbox" class="checkbox" checked={form().enabled} onChange={(e) => set('enabled', e.currentTarget.checked)} />
                <span class="label-text">启用</span>
              </label>
              <p class="text-xs text-base-content/70">
                Passkey 与 authkey 以明文存在 Graft 的数据库文件里，该文件只有属主能读。
              </p>
              <Show when={error()}>
                <div class="alert alert-error text-sm">{error()}</div>
              </Show>
              <div class="modal-action">
                <button type="button" class="btn btn-ghost" onClick={() => setEditing(null)}>取消</button>
                <button type="submit" class="btn btn-primary">保存</button>
              </div>
            </form>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Sites;
