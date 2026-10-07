import { Component, createSignal, createResource, For, Show } from 'solid-js';
import { fetchClients, createClient, testClient, deleteClient } from '../api/clients';

const Clients: Component = () => {
  const [clients, { refetch }] = createResource(fetchClients);
  const [showModal, setShowModal] = createSignal(false);
  const [testing, setTesting] = createSignal<string | null>(null);
  const [testResult, setTestResult] = createSignal<{ id: string; success: boolean; message: string } | null>(null);

  const [form, setForm] = createSignal({
    name: '',
    client_type: 'qbittorrent' as 'qbittorrent' | 'transmission',
    host: '',
    port: 8080,
    username: '',
    password: '',
    use_https: false,
  });

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    await createClient(form());
    setShowModal(false);
    setForm({
      name: '',
      client_type: 'qbittorrent',
      host: '',
      port: 8080,
      username: '',
      password: '',
      use_https: false,
    });
    refetch();
  };

  const handleTest = async (id: string) => {
    setTesting(id);
    setTestResult(null);
    const result = await testClient(id);
    setTestResult({ id, ...result });
    setTesting(null);
  };

  const handleDelete = async (id: string) => {
    if (confirm('确定删除这个下载器？')) {
      await deleteClient(id);
      refetch();
    }
  };

  return (
    <div>
      <div class="flex justify-between items-center mb-6">
        <h1 class="page-title mb-0">下载器</h1>
        <button class="btn btn-primary" onClick={() => setShowModal(true)}>
          添加下载器
        </button>
      </div>

      {/* Clients Table */}
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
                  <td>
                    <Show
                      when={testResult()?.id === client.id}
                      fallback={
                        <span class="badge badge-ghost">未测试</span>
                      }
                    >
                      <span class={`badge ${testResult()?.success ? 'badge-success' : 'badge-error'}`}>
                        {testResult()?.success ? '已连通' : '失败'}
                      </span>
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
                      <button
                        class="btn btn-sm btn-error btn-outline"
                        onClick={() => handleDelete(client.id)}
                      >
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

      {/* Add Client Modal */}
      <Show when={showModal()}>
        <div class="modal modal-open">
          <div class="modal-box">
            <h3 class="font-bold text-lg mb-4">添加下载器</h3>
            <form onSubmit={handleSubmit}>
              <div class="form-control mb-4">
                <label class="label">
                  <span class="label-text">名称</span>
                </label>
                <input
                  type="text"
                  class="input input-bordered"
                  value={form().name}
                  onInput={(e) => setForm({ ...form(), name: e.currentTarget.value })}
                  required
                />
              </div>

              <div class="form-control mb-4">
                <label class="label">
                  <span class="label-text">类型</span>
                </label>
                <select
                  class="select select-bordered"
                  value={form().client_type}
                  onChange={(e) => setForm({ ...form(), client_type: e.currentTarget.value as 'qbittorrent' | 'transmission' })}
                >
                  <option value="qbittorrent">qBittorrent</option>
                  <option value="transmission">Transmission</option>
                </select>
              </div>

              <div class="grid grid-cols-2 gap-4 mb-4">
                <div class="form-control">
                  <label class="label">
                    <span class="label-text">主机</span>
                  </label>
                  <input
                    type="text"
                    class="input input-bordered"
                    value={form().host}
                    onInput={(e) => setForm({ ...form(), host: e.currentTarget.value })}
                    placeholder="localhost"
                    required
                  />
                </div>
                <div class="form-control">
                  <label class="label">
                    <span class="label-text">端口</span>
                  </label>
                  <input
                    type="number"
                    class="input input-bordered"
                    value={form().port}
                    onInput={(e) => setForm({ ...form(), port: parseInt(e.currentTarget.value) })}
                    required
                  />
                </div>
              </div>

              <div class="grid grid-cols-2 gap-4 mb-4">
                <div class="form-control">
                  <label class="label">
                    <span class="label-text">用户名</span>
                  </label>
                  <input
                    type="text"
                    class="input input-bordered"
                    value={form().username}
                    onInput={(e) => setForm({ ...form(), username: e.currentTarget.value })}
                  />
                </div>
                <div class="form-control">
                  <label class="label">
                    <span class="label-text">密码</span>
                  </label>
                  <input
                    type="password"
                    class="input input-bordered"
                    value={form().password}
                    onInput={(e) => setForm({ ...form(), password: e.currentTarget.value })}
                  />
                </div>
              </div>

              <div class="form-control mb-4">
                <label class="label cursor-pointer">
                  <span class="label-text">使用 HTTPS</span>
                  <input
                    type="checkbox"
                    class="checkbox"
                    checked={form().use_https}
                    onChange={(e) => setForm({ ...form(), use_https: e.currentTarget.checked })}
                  />
                </label>
              </div>

              <p class="text-xs text-base-content/70 mb-4">
                密码以明文存在 Graft 的数据库文件里，该文件只有属主能读。
              </p>

              <div class="modal-action">
                <button type="button" class="btn" onClick={() => setShowModal(false)}>
                  取消
                </button>
                <button type="submit" class="btn btn-primary">
                  添加
                </button>
              </div>
            </form>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Clients;
