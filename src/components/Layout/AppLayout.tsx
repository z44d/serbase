import type { ReactNode } from 'react';
import Box from '@mui/material/Box';
import AppBar from '@mui/material/AppBar';
import Toolbar from '@mui/material/Toolbar';
import Typography from '@mui/material/Typography';
import IconButton from '@mui/material/IconButton';
import StorageIcon from '@mui/icons-material/Storage';

interface Props {
  sidebar: ReactNode;
  main: ReactNode;
}

export function AppLayout({ sidebar, main }: Props) {
  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100vh', width: '100vw' }}>
      <AppBar position="static" elevation={0}>
        <Toolbar variant="dense" sx={{ gap: 1, px: 1.5 }}>
          <IconButton size="small" disabled>
            <StorageIcon sx={{ color: 'primary.main' }} />
          </IconButton>
          <Typography variant="h6" fontWeight={700} letterSpacing="-0.02em">
            serbase
          </Typography>
          <Typography variant="caption" sx={{ color: 'text.secondary', ml: 0.5 }}>
            v0.1.0
          </Typography>
          <Box sx={{ flexGrow: 1 }} />
        </Toolbar>
      </AppBar>

      <Box sx={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        <Box
          sx={{
            width: 260,
            flexShrink: 0,
            borderRight: '1px solid',
            borderColor: 'divider',
            overflow: 'auto',
          }}
        >
          {sidebar}
        </Box>

        <Box sx={{ flex: 1, overflow: 'auto' }}>
          {main}
        </Box>
      </Box>
    </Box>
  );
}
