import { Component, createSignal, createResource, For, Show } from 'solid-js';
import {
  fetchClients,
  createClient,
  updateClient,
  testClient,
  deleteClient,
  type Client,
  type CreateClientRequest,
} from '../api/clients';

type ClientType = CreateClientRequest['client_type'];

const emptyForm = (): CreateClientRequest & { username: string; password: string; link_dir: string } => ({
  name: '',
  client_type: 'qbittorrent',
  host: '',
  port: 8080,
  username: '',
  password: '',
  use_https: false,
  link_dir: '',
});

type FormState = ReturnType<typeof emptyForm>;

const Clients: Component = () => {
  const [clients, { refetch }] = createResource(fetchClients);
  // null: closed; 'new': adding a client; otherwise the client being edited
  const [editing, setEditing] = createSignal<Client | 'new' | null>(null);
  const [form, setForm] = createSignal<FormState>(emptyForm());
  const [error, setError] = createSignal('');
  const [testing, setTesting] = createSignal<string | null>(null);
  const [testResult, setTestResult] = createSignal<{ id: string; success: boolean; message: string } | null>(null);

  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => setForm({ ...form(), [key]: value });
  const isNew = () => editing() === 'new';

  const openNew = () => {
    setForm(emptyForm());
    setError('');
    setEditing('new');
  };

  // The stored password is never sent back, so the field starts empty.
  const openEdit = (client: Client) => {
    setForm({
      ...emptyForm(),
      name: client.name,
      client_type: client.client_type,
      host: client.host,
      port: client.port,
      username: client.username ?? '',
      use_https: client.use_https,
      link_dir: client.link_dir ?? '',
    });
    setError('');
    setEditing(client);
  };

  const save = async (e: Event) => {
    e.preventDefault();
    const current = editing();
    try {
      if (current === 'new') {
        await createClient(form());
      } else if (current) {
        await updateClient(current.id, form());
        if (testResult()?.id === current.id) setTestResult(null);
      }
      setEditing(null);
      refetch();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const handleTest = async (id: string) => {
    setTesting(id);
    setTestResult(null);
    try {
      setTestResult({ id, ...(await testClient(id)) });
    } catch (err) {
      setTestResult({ id, success: false, message: (err as Error).message });
    } finally {
      setTesting(null);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm('确定删除这个下载器？')) return;
    try {
      await deleteClient(id);
      refetch();
    } catch (err) {
      alert((err as Error).message);
    }
  };

  return (
    <div>
      <div class="flex justify-between items-center mb-6">
        <h1 class="page-title mb-0">下载器</h1>
        <button class="btn btn-primary" onClick={openNew}>添加下载器</button>
      </div>

      <div class="table-container">
        <table class="table">
          <thead>
            <tr>
              <th>名称</th>
              <th>类型</th>
              <th>地址</th>
              <th>连通</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <For each={clients()}>
              {(client) => (
                <tr>
                  <td class="font-medium">{client.name}</td>
                  <td>
                    <span class="badge badge-outline">
                      {client.client_type === 'qbittorrent' ? 'qBittorrent' : 'Transmission'}
                    </span>
                  </td>
                  <td>
                    {client.use_https ? 'https' : 'http'}://{client.host}:{client.port}
                  </td>
                  <td class="max-w-xs">
                    <Show
                      when={testResult()?.id === client.id}
                      fallback={<span class="badge badge-ghost">未测试</span>}
                    >
                      <span class={`badge ${testResult()?.success ? 'badge-success' : 'badge-error'}`}>
                        {testResult()?.success ? '已连通' : '失败'}
                      </span>
                      <Show when={!testResult()?.success}>
                        <div class="text-xs text-error mt-1">{testResult()?.message}</div>
                      </Show>
                    </Show>
                  </td>
                  <td>
                    <div class="flex gap-2">
                      <button
                        class="btn btn-sm btn-outline"
                        onClick={() => handleTest(client.id)}
                        disabled={testing() === client.id}
                      >
                        {testing() === client.id ? (
                          <span class="loading loading-spinner loading-xs"></span>
                        ) : (
                          '测试'
                        )}
                      </button>
                      <button class="btn btn-sm btn-ghost" onClick={() => openEdit(client)}>编辑</button>
                      <button class="btn btn-sm btn-error btn-outline" onClick={() => handleDelete(client.id)}>
                        删除
                      </button>
                    </div>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      <Show when={editing()}>
        <div class="modal modal-open">
          <div class="modal-box">
            <h3 class="font-bold text-lg mb-4">{isNew() ? '添加下载器' : `编辑 ${form().name}`}</h3>
            <form onSubmit={save} class="space-y-3">
              <label class="form-control">
                <span class="label-text">名称</span>
                <input class="input input-bordered" value={form().name}
                  onInput={(e) => set('name', e.currentTarget.value)} required />
              </label>

              <label class="form-control">
                <span class="label-text">类型</span>
                <select class="select select-bordered" value={form().client_type}
                  onChange={(e) => set('client_type', e.currentTarget.value as ClientType)}>
                  <option value="qbittorrent">qBittorrent</option>
                  <option value="transmission">Transmission</option>
                </select>
              </label>

              <div class="grid grid-cols-2 gap-3">
                <label class="form-control">
                  <span class="label-text">主机</span>
                  <input class="input input-bordered" value={form().host} placeholder="localhost"
                    onInput={(e) => set('host', e.currentTarget.value)} required />
                </label>
                <label class="form-control">
                  <span class="label-text">端口</span>
                  <input type="number" class="input input-bordered" value={form().port}
                    onInput={(e) => set('port', parseInt(e.currentTarget.value))} required />
                </label>
              </div>

              <div class="grid grid-cols-2 gap-3">
                <label class="form-control">
                  <span class="label-text">用户名</span>
                  <input class="input input-bordered" value={form().username}
                    onInput={(e) => set('username', e.currentTarget.value)} />
                </label>
                <label class="form-control">
                  <span class="label-text">密码{isNew() ? '' : '（留空则保留已存的）'}</span>
                  <input type="password" class="input input-bordered" value={form().password} autocomplete="off"
                    onInput={(e) => set('password', e.currentTarget.value)} />
                </label>
              </div>

              <label class="label cursor-pointer justify-start gap-3">
                <input type="checkbox" class="checkbox" checked={form().use_https}
                  onChange={(e) => set('use_https', e.currentTarget.checked)} />
                <span class="label-text">使用 HTTPS</span>
              </label>

              <label class="form-control">
                <span class="label-text">硬链接目录（可选）</span>
                <input class="input input-bordered font-mono text-sm" value={form().link_dir}
                  placeholder="/downloads/graft-links" onInput={(e) => set('link_dir', e.currentTarget.value)} />
                <span class="label-text-alt mt-1">
                  文件名不同的种子在这里建硬链接。写下载器里看到的路径；Graft 要能按同一路径访问到数据，
                  且这个目录必须和数据在同一个文件系统（ZFS 数据集）。别放进媒体库目录，以免被当成重复影片。
                </span>
              </label>

              <p class="text-xs text-base-content/70">
                密码以明文存在 Graft 的数据库文件里，该文件只有属主能读。
              </p>
              <Show when={error()}>
                <div class="alert alert-error text-sm">{error()}</div>
              </Show>
              <div class="modal-action">
                <button type="button" class="btn btn-ghost" onClick={() => setEditing(null)}>取消</button>
                <button type="submit" class="btn btn-primary">{isNew() ? '添加' : '保存'}</button>
              </div>
            </form>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Clients;
