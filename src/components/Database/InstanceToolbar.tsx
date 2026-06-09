import { useState, useCallback } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Tooltip from '@mui/material/Tooltip';
import Tab from '@mui/material/Tab';
import Tabs from '@mui/material/Tabs';
import Typography from '@mui/material/Typography';
import TextField from '@mui/material/TextField';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import IconButton from '@mui/material/IconButton';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import SettingsIcon from '@mui/icons-material/Settings';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import StopIcon from '@mui/icons-material/Stop';
import DeleteSweepIcon from '@mui/icons-material/DeleteSweep';
import TerminalIcon from '@mui/icons-material/Terminal';
import ArticleIcon from '@mui/icons-material/Article';
import { StatusBadge } from '../Common/StatusBadge';
import { useDatabaseStore } from '../../store/database-store';
import type { InstanceState } from '../../database/types';

interface Props {
  instance: InstanceState;
  bottomTab: 'logs' | 'terminal';
  onBottomTabChange: (tab: 'logs' | 'terminal') => void;
}

export function InstanceToolbar({ instance, bottomTab, onBottomTabChange }: Props) {
  const startServer = useDatabaseStore((s) => s.startServer);
  const stopServer = useDatabaseStore((s) => s.stopServer);
  const wipeServer = useDatabaseStore((s) => s.wipeServer);
  const serverDefs = useDatabaseStore((s) => s.serverDefs);
  const def = serverDefs.get(instance.id);

  const [configOpen, setConfigOpen] = useState(false);
  const [configHost, setConfigHost] = useState(instance.host);
  const [configPort, setConfigPort] = useState(String(instance.port));
  const [urlCopied, setUrlCopied] = useState(false);

  const isRunning = instance.status === 'running';

  const handleSaveConfig = () => {
    setConfigOpen(false);
  };

  const db = instance.database || def?.username || 'postgres';
  const connUrl = `postgresql://${def?.username || 'postgres'}${def?.password ? `:${def.password}` : ''}@${instance.host}:${instance.port}/${db}`;

  const handleCopyUrl = useCallback(() => {
    navigator.clipboard.writeText(connUrl).then(() => {
      setUrlCopied(true);
      setTimeout(() => setUrlCopied(false), 2000);
    });
  }, [connUrl]);

  return (
    <Box
      sx={{
        px: 2.5,
        py: 1.5,
        borderBottom: '1px solid',
        borderColor: 'divider',
        display: 'flex',
        alignItems: 'center',
        gap: 2,
        flexWrap: 'wrap',
      }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, minWidth: 0 }}>
        <Typography variant="h6" fontWeight={700} noWrap>
          {instance.name || instance.label}
        </Typography>
        <StatusBadge status={instance.status} size="medium" />
        <Typography
          variant="caption"
          color="text.secondary"
          sx={{ fontFamily: '"JetBrains Mono", monospace' }}
        >
          {instance.host}:{instance.port}
        </Typography>
        {isRunning && instance.type === 'postgres' && (
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, minWidth: 0 }}>
            <Typography
              variant="caption"
              color="success.main"
              sx={{ fontFamily: '"JetBrains Mono", monospace', fontSize: '0.65rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: { xs: 180, sm: 300 } }}
            >
              {connUrl}
            </Typography>
            <Tooltip title={urlCopied ? 'Copied!' : 'Copy connection URL'}>
              <IconButton size="small" onClick={handleCopyUrl} sx={{ color: urlCopied ? 'success.main' : 'text.secondary', p: 0.25 }}>
                <ContentCopyIcon sx={{ fontSize: 12 }} />
              </IconButton>
            </Tooltip>
          </Box>
        )}
        {!isRunning && (
          <Tooltip title="Configure host and port">
            <Button
              size="small"
              variant="text"
              color="inherit"
              sx={{ minWidth: 28, px: 0.5 }}
              onClick={() => {
                setConfigHost(instance.host);
                setConfigPort(String(instance.port));
                setConfigOpen(true);
              }}
            >
              <SettingsIcon fontSize="small" />
            </Button>
          </Tooltip>
        )}
      </Box>

      <Box sx={{ display: 'flex', gap: 1, ml: 'auto' }}>
        {!isRunning ? (
          <Tooltip title="Start database instance">
            <Button
              size="small"
              variant="contained"
              color="success"
              startIcon={<PlayArrowIcon />}
              onClick={() => startServer(instance.id)}
              disabled={instance.status === 'starting'}
            >
              Start
            </Button>
          </Tooltip>
        ) : (
          <Tooltip title="Stop database instance">
            <Button
              size="small"
              variant="outlined"
              color="error"
              startIcon={<StopIcon />}
              onClick={() => stopServer(instance.id)}
            >
              Stop
            </Button>
          </Tooltip>
        )}

        <Tooltip title="Wipe all data and reset">
          <Button
            size="small"
            variant="text"
            color="warning"
            startIcon={<DeleteSweepIcon />}
            onClick={() => wipeServer(instance.id)}
          >
            Wipe
          </Button>
        </Tooltip>

        <Box sx={{ ml: 2, borderLeft: '1px solid', borderColor: 'divider', pl: 2 }}>
          <Tabs
            value={bottomTab}
            onChange={(_, v) => onBottomTabChange(v)}
            sx={{
              minHeight: 36,
              '& .MuiTab-root': { minHeight: 36, py: 0.5, textTransform: 'none' },
            }}
          >
            <Tab
              icon={<ArticleIcon sx={{ fontSize: 16 }} />}
              iconPosition="start"
              label="Logs"
              value="logs"
              sx={{ fontSize: '0.8rem' }}
            />
            <Tab
              icon={<TerminalIcon sx={{ fontSize: 16 }} />}
              iconPosition="start"
              label="Terminal"
              value="terminal"
              sx={{ fontSize: '0.8rem' }}
            />
          </Tabs>
        </Box>
      </Box>

      <Dialog open={configOpen} onClose={() => setConfigOpen(false)} maxWidth="xs" fullWidth>
        <DialogTitle>Configure {instance.name || instance.label}</DialogTitle>
        <DialogContent>
          <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, pt: 1 }}>
            <TextField
              label="Host"
              size="small"
              value={configHost}
              onChange={(e) => setConfigHost(e.target.value)}
              placeholder="127.0.0.1"
              disabled={isRunning}
            />
            <TextField
              label="Port"
              size="small"
              value={configPort}
              onChange={(e) => setConfigPort(e.target.value)}
              type="number"
              disabled={isRunning}
            />
          </Box>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setConfigOpen(false)}>Cancel</Button>
          <Button variant="contained" onClick={handleSaveConfig} disabled={isRunning}>
            Save
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
}
