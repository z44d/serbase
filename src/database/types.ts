export type DBType = 'postgres' | 'redis' | 'mongo';

export type DBStatus = 'stopped' | 'starting' | 'running' | 'error';

export interface LogEntry {
  timestamp: number;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
}

export interface QueryResult {
  columns: string[];
  rows: Record<string, unknown>[];
  rowCount: number;
  command: string;
}

export interface ServerDefinition {
  id: string;
  type: DBType;
  name: string;
  host: string;
  port: number;
  username: string;
  password: string;
}

export interface InstanceState {
  id: string;
  type: DBType;
  name: string;
  label: string;
  host: string;
  port: number;
  status: DBStatus;
  uptime: number;
  logs: LogEntry[];
  connectionCount: number;
  dataSize: string;
}
