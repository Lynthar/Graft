import { Component, createSignal, createResource, For, Show } from 'solid-js';
import { fetchSites, fetchAvailableSites, createSite, deleteSite, type Site, type AvailableSite } from '../api/sites';

const Sites: Component = () => {
  const [sites, { refetch }] = createResource(fetchSites);
  const [availableSites] = createResource(fetchAvailableSites);
  const [showModal, setShowModal] = createSignal(false);

  const [form, setForm] = createSignal({
    id: '',
    name: '',
    passkey: '',
    cookie: '',
  });

  const [selectedTemplate, setSelectedTemplate] = createSignal<AvailableSite | null>(null);

  const handleSelectTemplate = (template: AvailableSite) => {
    setSelectedTemplate(template);
    setForm({
      id: template.id,
      name: template.name,
      passkey: '',
      cookie: '',
    });
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    const template = selectedTemplate();
    if (!template) return;

    await createSite({
      id: form().id,
      name: form().name,
      base_url: template.base_url,
      passkey: form().passkey || undefined,
      cookie: form().cookie || undefined,
    });

    setShowModal(false);
    setSelectedTemplate(null);
    setForm({ id: '', name: '', passkey: '', cookie: '' });
    refetch();
  };

  const handleDelete = async (id: string) => {
    if (confirm('Are you sure you want to delete this site?')) {
      await deleteSite(id);
      refetch();
    }
  };

  return (
    <div>
      <div class="flex justify-between items-center mb-6">
        <h1 class="page-title mb-0">PT Sites</h1>
        <button class="btn btn-primary" onClick={() => setShowModal(true)}>
          Add Site
        </button>
      </div>

      {/* Sites Table */}
      <div class="table-container">
        <table class="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>ID</th>
              <th>Template</th>
              <th>Passkey</th>
              <th>Status</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <For each={sites()}>
              {(site) => (
                <tr>
                  <td class="font-medium">{site.name}</td>
                  <td><code class="text-xs">{site.id}</code></td>
                  <td>
                    <span class="badge badge-outline">{site.template_type}</span>
                  </td>
                  <td>
                    {site.has_passkey ? (
                      <span class="badge badge-success badge-sm">Configured</span>
                    ) : (
                      <span class="badge badge-warning badge-sm">Missing</span>
                    )}
                  </td>
                  <td>
                    <span class={`badge ${site.enabled ? 'badge-success' : 'badge-ghost'}`}>
                      {site.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                  </td>
                  <td>
                    <button
                      class="btn btn-sm btn-error btn-outline"
                      onClick={() => handleDelete(site.id)}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      {/* Add Site Modal */}
      <Show when={showModal()}>
        <div class="modal modal-open">
          <div class="modal-box max-w-3xl">
            <h3 class="font-bold text-lg mb-4">Add PT Site</h3>

            <Show
              when={selectedTemplate()}
              fallback={
                <div>
                  <p class="mb-4 text-base-content/70">Select a site template:</p>
                  <div class="grid grid-cols-2 md:grid-cols-3 gap-4">
                    <For each={availableSites()}>
                      {(site) => (
                        <button
                          class="btn btn-outline h-auto py-4 flex-col"
                          onClick={() => handleSelectTemplate(site)}
                        >
                          <span class="font-bold">{site.name}</span>
                          <span class="text-xs opacity-70">{site.template_type}</span>
                        </button>
                      )}
                    </For>
                  </div>
                  <div class="modal-action">
                    <button class="btn" onClick={() => setShowModal(false)}>
                      Cancel
                    </button>
                  </div>
                </div>
              }
            >
              <form onSubmit={handleSubmit}>
                <div class="alert alert-info mb-4">
                  <span>Configuring: <strong>{selectedTemplate()?.name}</strong></span>
                </div>

                <div class="form-control mb-4">
                  <label class="label">
                    <span class="label-text">Passkey</span>
                  </label>
                  <input
                    type="text"
                    class="input input-bordered"
                    value={form().passkey}
                    onInput={(e) => setForm({ ...form(), passkey: e.currentTarget.value })}
                    placeholder="Your passkey from the site"
                  />
                  <label class="label">
                    <span class="label-text-alt">Find this in your site profile settings</span>
                  </label>
                </div>

                <div class="form-control mb-4">
                  <label class="label">
                    <span class="label-text">Cookie (optional)</span>
                  </label>
                  <textarea
                    class="textarea textarea-bordered"
                    value={form().cookie}
                    onInput={(e) => setForm({ ...form(), cookie: e.currentTarget.value })}
                    placeholder="Cookie string (for sites that require it)"
                    rows={2}
                  />
                </div>

                <div class="modal-action">
                  <button type="button" class="btn" onClick={() => {
                    setSelectedTemplate(null);
                    setShowModal(false);
                  }}>
                    Cancel
                  </button>
                  <button type="button" class="btn btn-ghost" onClick={() => setSelectedTemplate(null)}>
                    Back
                  </button>
                  <button type="submit" class="btn btn-primary">
                    Add Site
                  </button>
                </div>
              </form>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default Sites;
