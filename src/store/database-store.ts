import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Store } from '@tauri-apps/plugin-store';
import type { DBType, DBStatus, LogEntry, InstanceState, ServerDefinition } from '../database/types';

const STORE_PATH = 'serbase-servers.json';
const LS_KEY = 'serbase-serverDefs';

async function getStore(): Promise<Store> {
  return await Store.load(STORE_PATH);
}

function saveToLocal(defs: Map<string, ServerDefinition>): void {
  try {
    const obj: Record<string, ServerDefinition> = {};
    for (const [id, def] of defs) {
      obj[id] = def;
    }
    localStorage.setItem(LS_KEY, JSON.stringify(obj));
  } catch (e) {
    console.warn('Failed to save to localStorage:', e);
  }
}

function loadFromLocal(): Record<string, ServerDefinition> | null {
  try {
    const data = localStorage.getItem(LS_KEY);
    return data ? JSON.parse(data) : null;
  } catch (e) {
    console.warn('Failed to load from localStorage:', e);
    return null;
  }
}

let serverIdCounter = 0;
function generateServerId(): string {
  serverIdCounter += 1;
  return `server-${Date.now()}-${serverIdCounter}`;
}

interface DatabaseStore {
  instances: Map<string, InstanceState>;
  serverDefs: Map<string, ServerDefinition>;
  activeInstanceId: string | null;

  initialize: () => Promise<void>;
  createServer: (def: Omit<ServerDefinition, 'id'>) => Promise<string>;
  removeServer: (id: string) => Promise<void>;
  startServer: (id: string) => Promise<void>;
  stopServer: (id: string) => Promise<void>;
  wipeServer: (id: string) => Promise<void>;
  executeQuery: (id: string, query: string) => Promise<string>;
  setActiveInstance: (id: string | null) => void;
}

const TYPE_LABELS: Record<DBType, string> = {
  postgres: 'PostgreSQL',
  redis: 'Redis',
  mongo: 'MongoDB',
};

function makeLabel(def: ServerDefinition): string {
  return def.name || `${TYPE_LABELS[def.type]} (${def.host}:${def.port})`;
}

function defsToObject(defs: Map<string, ServerDefinition>): Record<string, ServerDefinition> {
  const obj: Record<string, ServerDefinition> = {};
  for (const [id, def] of defs) {
    obj[id] = def;
  }
  return obj;
}

async function persistDefs(defs: Map<string, ServerDefinition>): Promise<void> {
  saveToLocal(defs);
  try {
    const store = await getStore();
    await store.set('serverDefs', defsToObject(defs));
    await store.save();
  } catch (e) {
    console.warn('Failed to persist server configs:', e);
  }
}

export const useDatabaseStore = create<DatabaseStore>((set, get) => ({
  instances: new Map(),
  serverDefs: new Map(),
  activeInstanceId: null,

  initialize: async () => {
    try {
      await listen<{ db_type: string; message: string; level: string }>('db:log', (event) => {
        const { db_type, message } = event.payload;
        addLogToInstance(db_type, { timestamp: Date.now(), level: 'info', message });
      });

      await listen<{ db_type: string; message: string; level: string }>('db:debug', (event) => {
        const { db_type, message } = event.payload;
        addLogToInstance(db_type, { timestamp: Date.now(), level: 'debug', message });
      });

      await listen<{ db_type: string; running: boolean; port: number; host: string; name: string; username: string }>('db:status', (event) => {
        const { db_type, running, port, host } = event.payload;
        updateInstanceStatus(db_type, running, port, host);
      });

      let savedDefs = loadFromLocal();
      try {
        const store = await getStore();
        const storeDefs = await store.get<Record<string, ServerDefinition>>('serverDefs');
        if (storeDefs) {
          savedDefs ??= {};
          Object.assign(savedDefs, storeDefs);
        }
      } catch (e) {
        console.warn('Failed to load from store plugin:', e);
      }
      if (savedDefs) {
        const defs = new Map<string, ServerDefinition>();
        const instances = new Map<string, InstanceState>();
        for (const [id, def] of Object.entries(savedDefs)) {
          defs.set(id, def);
          instances.set(id, {
            id,
            type: def.type,
            name: def.name,
            label: makeLabel(def),
            host: def.host,
            port: def.port,
            database: def.database,
            status: 'stopped',
            uptime: 0,
            logs: [],
            connectionCount: 0,
            dataSize: '0 B',
          });
        }
        set({ serverDefs: defs, instances });
      }
    } catch (e) {
      console.warn('Failed to set up event listeners or load configs:', e);
    }
  },

  createServer: async (def) => {
    const id = generateServerId();
    const serverDef: ServerDefinition = { id, ...def };

    const instance: InstanceState = {
      id,
      type: def.type,
      name: def.name,
      label: makeLabel(serverDef),
      host: def.host,
      port: def.port,
      database: def.database,
      status: 'stopped',
      uptime: 0,
      logs: [],
      connectionCount: 0,
      dataSize: '0 B',
    };

    const state = get();
    const newDefs = new Map(state.serverDefs);
    newDefs.set(id, serverDef);
    const newInstances = new Map(state.instances);
    newInstances.set(id, instance);
    set({ serverDefs: newDefs, instances: newInstances, activeInstanceId: id });

    await persistDefs(newDefs);

    return id;
  },

  removeServer: async (id) => {
    const state = get();
    const instance = state.instances.get(id);
    if (instance && instance.status === 'running') {
      try { await invoke('stop_database', { serverId: id }); } catch {}
    }
    const newDefs = new Map(state.serverDefs);
    newDefs.delete(id);
    const newInstances = new Map(state.instances);
    newInstances.delete(id);
    const activeId = state.activeInstanceId === id ? null : state.activeInstanceId;
    set({ serverDefs: newDefs, instances: newInstances, activeInstanceId: activeId });

    await persistDefs(newDefs);
  },

  startServer: async (id) => {
    const state = get();
    const def = state.serverDefs.get(id);
    if (!def) return;

    const instance = state.instances.get(id);
    if (!instance) return;

    const updated = { ...instance, status: 'starting' as DBStatus };
    const newInstances = new Map(state.instances);
    newInstances.set(id, updated);
    set({ instances: newInstances });

    try {
      await invoke('create_database', {
        serverId: id,
        dbType: def.type,
        host: def.host,
        port: def.port,
        name: def.name,
        username: def.username,
        password: def.password,
        database: def.database,
      });
      const running = { ...updated, status: 'running' as DBStatus, uptime: Date.now() };
      const runningInstances = new Map(get().instances);
      runningInstances.set(id, running);
      set({ instances: runningInstances, activeInstanceId: id });
    } catch (e) {
      console.error('Failed to start server:', e);
      const failed = { ...updated, status: 'error' as DBStatus };
      const failedInstances = new Map(get().instances);
      failedInstances.set(id, failed);
      set({ instances: failedInstances });
    }
  },

  stopServer: async (id) => {
    try {
      await invoke('stop_database', { serverId: id });
    } catch (e) {
      console.warn('stop_database failed:', e);
    }
    const state = get();
    const instance = state.instances.get(id);
    if (instance) {
      const updated = { ...instance, status: 'stopped' as DBStatus, uptime: 0 };
      const newInstances = new Map(state.instances);
      newInstances.set(id, updated);
      set({ instances: newInstances });
    }
  },

  wipeServer: async (id) => {
    try {
      await invoke('wipe_database', { serverId: id });
    } catch (e) {
      console.warn('wipe_database failed:', e);
    }
    const state = get();
    const instance = state.instances.get(id);
    if (instance) {
      const updated = { ...instance, status: 'stopped' as DBStatus, uptime: 0, logs: [] };
      const newInstances = new Map(state.instances);
      newInstances.set(id, updated);
      set({ instances: newInstances });
    }
  },

  executeQuery: async (id, query) => {
    return await invoke<string>('execute_query', { serverId: id, query });
  },

  setActiveInstance: (id) => {
    set({ activeInstanceId: id });
  },
}));

function addLogToInstance(id: string, entry: LogEntry) {
  const state = useDatabaseStore.getState();
  const instance = state.instances.get(id);
  if (instance) {
    const logs = [...instance.logs, entry].slice(-1000);
    const updated = { ...instance, logs };
    const newInstances = new Map(state.instances);
    newInstances.set(id, updated);
    useDatabaseStore.setState({ instances: newInstances });
  }
}

function updateInstanceStatus(id: string, running: boolean, port: number, host: string) {
  const state = useDatabaseStore.getState();
  const instance = state.instances.get(id);
  if (instance) {
    const updated: InstanceState = {
      ...instance,
      status: running ? 'running' as DBStatus : 'stopped' as DBStatus,
      port: port || instance.port,
      host: host || instance.host,
      uptime: running ? Date.now() : 0,
    };
    const newInstances = new Map(state.instances);
    newInstances.set(id, updated);
    useDatabaseStore.setState({ instances: newInstances });
  }
}
