import { useState, useRef, useEffect, useCallback } from 'react';
import Box from '@mui/material/Box';
import { useDatabaseStore } from '../../store/database-store';
import { EmptyState } from '../Common/EmptyState';
import { InstanceToolbar } from '../Database/InstanceToolbar';
import { DataBrowser } from '../Database/DataBrowser';
import { LogViewer } from '../Database/LogViewer';
import { DatabaseTerminal } from '../Database/DatabaseTerminal';

const MIN_BOTTOM = 100;
const MAX_BOTTOM_PERCENT = 0.6;

export function InstancePanel() {
  const activeInstanceId = useDatabaseStore((s) => s.activeInstanceId);
  const instance = useDatabaseStore((s) => (activeInstanceId ? s.instances.get(activeInstanceId) : undefined));
  const [bottomTab, setBottomTab] = useState<'logs' | 'terminal'>('logs');
  const [bottomHeight, setBottomHeight] = useState(240);
  const containerRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!dragging.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const newHeight = rect.bottom - e.clientY;
      const maxH = rect.height * MAX_BOTTOM_PERCENT;
      setBottomHeight(Math.max(MIN_BOTTOM, Math.min(newHeight, maxH)));
    };
    const handleMouseUp = () => {
      dragging.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, []);

  if (!instance) {
    return <EmptyState />;
  }

  return (
    <Box ref={containerRef} sx={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <InstanceToolbar instance={instance} bottomTab={bottomTab} onBottomTabChange={setBottomTab} />

      <Box sx={{ flex: 1, overflow: 'auto', p: 2 }}>
        <DataBrowser instance={instance} />
      </Box>

      <Box
        onMouseDown={handleMouseDown}
        sx={{
          height: 4,
          cursor: 'row-resize',
          bgcolor: 'divider',
          '&:hover': { bgcolor: 'primary.main', opacity: 0.5 },
          flexShrink: 0,
        }}
      />

      <Box
        sx={{
          height: bottomHeight,
          flexShrink: 0,
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
