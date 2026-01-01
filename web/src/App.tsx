import { Component, lazy } from 'solid-js';
import { Router, Route } from '@solidjs/router';
import Layout from './components/Layout';

// Lazy load pages
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Clients = lazy(() => import('./pages/Clients'));
const Sites = lazy(() => import('./pages/Sites'));
const Index = lazy(() => import('./pages/Index'));
const Reseed = lazy(() => import('./pages/Reseed'));
const History = lazy(() => import('./pages/History'));

const App: Component = () => {
  return (
    <Router root={Layout}>
      <Route path="/" component={Dashboard} />
      <Route path="/clients" component={Clients} />
      <Route path="/sites" component={Sites} />
      <Route path="/index" component={Index} />
      <Route path="/reseed" component={Reseed} />
      <Route path="/history" component={History} />
    </Router>
  );
};

export default App;
