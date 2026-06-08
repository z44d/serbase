import Box from '@mui/material/Box';
import { useDatabaseStore } from '../../store/database-store';
import { EmptyState } from '../Common/EmptyState';
import { InstanceToolbar } from '../Database/InstanceToolbar';
import { DataBrowser } from '../Database/DataBrowser';
import { LogViewer } from '../Database/LogViewer';
import { DatabaseTerminal } from '../Database/DatabaseTerminal';
import { useState } from 'react';

export function InstancePanel() {
  const activeInstanceId = useDatabaseStore((s) => s.activeInstanceId);
  const instance = useDatabaseStore((s) => (activeInstanceId ? s.instances.get(activeInstanceId) : undefined));
  const [bottomTab, setBottomTab] = useState<'logs' | 'terminal'>('logs');

  if (!instance) {
    return <EmptyState />;
  }

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <InstanceToolbar instance={instance} bottomTab={bottomTab} onBottomTabChange={setBottomTab} />

      <Box sx={{ flex: 1, overflow: 'auto', p: 2 }}>
        <DataBrowser instance={instance} />
      </Box>

      <Box
        sx={{
          height: 240,
          borderTop: '1px solid',
          borderColor: 'divider',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {bottomTab === 'logs' ? (
          <LogViewer instance={instance} />
        ) : (
          <DatabaseTerminal instance={instance} />
        )}
      </Box>
    </Box>
  );
}
