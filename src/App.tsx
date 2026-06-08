import { useEffect } from 'react';
import { Box } from '@mui/material';
import { useDatabaseStore } from './store/database-store';
import { AppLayout } from './components/Layout/AppLayout';
import { DatabaseSidebar } from './components/Layout/DatabaseSidebar';
import { InstancePanel } from './components/Layout/InstancePanel';

export default function App() {
  const initialize = useDatabaseStore((s) => s.initialize);

  useEffect(() => {
    initialize();
  }, [initialize]);

  return (
    <Box sx={{ display: 'flex', height: '100vh', overflow: 'hidden' }}>
      <AppLayout
        sidebar={<DatabaseSidebar />}
        main={<InstancePanel />}
      />
    </Box>
  );
}
