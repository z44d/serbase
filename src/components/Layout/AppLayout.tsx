import AppBar from "@mui/material/AppBar";
import Box from "@mui/material/Box";
import Dialog from "@mui/material/Dialog";
import DialogContent from "@mui/material/DialogContent";
import DialogTitle from "@mui/material/DialogTitle";
import Drawer from "@mui/material/Drawer";
import IconButton from "@mui/material/IconButton";
import Link from "@mui/material/Link";
import MenuIcon from "@mui/icons-material/Menu";
import OpenInNewIcon from "@mui/icons-material/OpenInNew";
import { openUrl } from '@tauri-apps/plugin-opener';
import StorageIcon from "@mui/icons-material/Storage";
import Toolbar from "@mui/material/Toolbar";
import Typography from "@mui/material/Typography";
import { useMediaQuery, useTheme } from "@mui/material";
import { useState } from "react";
import type { ReactNode } from "react";

interface Props {
  sidebar: ReactNode;
  main: ReactNode;
}

export function AppLayout({ sidebar, main }: Props) {
  const theme = useTheme();
  const isMobile = useMediaQuery(theme.breakpoints.down('md'));
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100dvh', width: '100vw', overflow: 'hidden' }}>
      <AppBar position="static" elevation={0}>
        <Toolbar variant="dense" sx={{ gap: 1, px: { xs: 1, sm: 1.5 } }}>
          {isMobile && (
            <IconButton size="small" onClick={() => setDrawerOpen(true)}>
              <MenuIcon />
            </IconButton>
          )}
          <IconButton size="small" onClick={() => setAboutOpen(true)}>
            <StorageIcon sx={{ color: 'primary.main' }} />
          </IconButton>
          <Typography variant="h6" fontWeight={700} letterSpacing="-0.02em" sx={{ fontSize: { xs: '1rem', sm: '1.25rem' } }}>
            SerBase
          </Typography>
          <Typography variant="caption" sx={{ color: 'text.secondary', ml: 0.5, display: { xs: 'none', sm: 'block' } }}>
            v0.1.1
          </Typography>
          <Box sx={{ flexGrow: 1 }} />
        </Toolbar>
      </AppBar>

      <Box sx={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        {isMobile ? (
          <Drawer open={drawerOpen} onClose={() => setDrawerOpen(false)} sx={{ '& .MuiDrawer-paper': { width: 260 } }}>
            {sidebar}
          </Drawer>
        ) : (
          <Box
            sx={{
              width: 260,
              flexShrink: 0,
              borderRight: '1px solid',
              borderColor: 'divider',
              overflow: 'auto',
              display: { xs: 'none', md: 'block' },
            }}
          >
            {sidebar}
          </Box>
        )}

        <Box sx={{ flex: 1, overflow: 'auto', minWidth: 0 }}>
          {main}
        </Box>
      </Box>

      <Dialog open={aboutOpen} onClose={() => setAboutOpen(false)} maxWidth="xs" fullWidth>
        <DialogTitle sx={{ display: 'flex', alignItems: 'center', gap: 1.5 }}>
          <StorageIcon sx={{ color: 'primary.main' }} />
          <span>SerBase</span>
        </DialogTitle>
        <DialogContent>
          <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
            Local database manager. v0.1.1
          </Typography>
          <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
            <Link
              component="button"
              onClick={() => openUrl('https://github.com/z44d/serbase')}
              underline="hover"
              sx={{ display: 'flex', alignItems: 'center', gap: 1, cursor: 'pointer', textAlign: 'left', background: 'none', border: 'none', color: 'primary.main', fontSize: 'inherit', p: 0, fontFamily: 'inherit' }}
            >
              <OpenInNewIcon sx={{ fontSize: 16 }} /> GitHub
            </Link>
            <Link
              component="button"
              onClick={() => openUrl('https://t.me/zaidlab')}
              underline="hover"
              sx={{ display: 'flex', alignItems: 'center', gap: 1, cursor: 'pointer', textAlign: 'left', background: 'none', border: 'none', color: 'primary.main', fontSize: 'inherit', p: 0, fontFamily: 'inherit' }}
            >
              <OpenInNewIcon sx={{ fontSize: 16 }} /> Telegram
            </Link>
          </Box>
        </DialogContent>
      </Dialog>
    </Box>
  );
}
