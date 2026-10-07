import { Component, createSignal, Show } from 'solid-js';
import { login } from '../api/auth';

/** Where to go after logging in: only a path on this site, never another origin. */
const nextPath = () => {
  const next = new URLSearchParams(location.search).get('next');
  return next && next.startsWith('/') && !next.startsWith('//') ? next : '/';
};

const Login: Component = () => {
  const [password, setPassword] = createSignal('');
  const [error, setError] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError('');
    try {
      await login(password());
      location.assign(nextPath());
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="flex min-h-screen items-center justify-center bg-base-300 p-4">
      <form class="card bg-base-100 shadow-xl w-full max-w-sm" onSubmit={submit}>
        <div class="card-body space-y-4">
          <h1 class="text-2xl font-bold text-primary">🌿 Graft</h1>
          <label class="form-control">
            <span class="label-text">Password</span>
            <input type="password" class="input input-bordered" autocomplete="current-password" autofocus
              value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
          </label>
          <Show when={error()}>
            <div class="alert alert-error text-sm">{error()}</div>
          </Show>
          <button type="submit" class="btn btn-primary" disabled={busy() || !password()}>Log in</button>
          <p class="text-xs text-base-content/70">You stay logged in on this device for 30 days after you last used it.</p>
        </div>
      </form>
    </div>
  );
};

export default Login;
