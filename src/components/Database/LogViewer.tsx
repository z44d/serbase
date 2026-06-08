import { useRef, useEffect, useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import Switch from '@mui/material/Switch';
import FormControlLabel from '@mui/material/FormControlLabel';
import type { InstanceState, LogEntry } from '../../database/types';

interface Props {
  instance: InstanceState;
}

const levelColors: Record<LogEntry['level'], string> = {
  info: '#66bb6a',
  warn: '#ffa726',
  error: '#f44336',
  debug: '#90a4ae',
};

export function LogViewer({ instance }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [showDebug, setShowDebug] = useState(false);

  const filteredLogs = showDebug
    ? instance.logs
    : instance.logs.filter((entry) => entry.level !== 'debug');

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [filteredLogs.length]);

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <Box
        sx={{
          px: 2,
          py: 0.75,
          borderBottom: '1px solid',
          borderColor: 'divider',
          display: 'flex',
          alignItems: 'center',
          gap: 1,
        }}
      >
        <Typography variant="caption" fontWeight={700} color="text.secondary">
          LOG OUTPUT
        </Typography>
        <Typography variant="caption" color="text.secondary">
          ({filteredLogs.length} entries)
        </Typography>
        <Box sx={{ ml: 'auto' }}>
          <FormControlLabel
            control={
              <Switch
                size="small"
                checked={showDebug}
                onChange={(e) => setShowDebug(e.target.checked)}
              />
            }
            label={
              <Typography variant="caption" color="text.secondary">
                Debug
              </Typography>
            }
            sx={{ m: 0 }}
          />
        </Box>
      </Box>

      <Box
        sx={{
          flex: 1,
          overflow: 'auto',
          p: 1.5,
          bgcolor: '#060d17',
          fontFamily: '"JetBrains Mono", "Fira Code", monospace',
          fontSize: '0.75rem',
          lineHeight: 1.6,
        }}
      >
        {filteredLogs.length === 0 ? (
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ fontFamily: 'inherit', opacity: 0.5 }}
          >
            No log entries yet. Start the database to begin logging.
          </Typography>
        ) : (
          filteredLogs.map((entry, i) => (
            <Box
              key={i}
              sx={{
                display: 'flex',
                gap: 1.5,
                '&:hover': { bgcolor: 'rgba(255,255,255,0.02)' },
                borderRadius: 0.5,
                px: 0.5,
              }}
            >
              <Typography
                variant="caption"
                sx={{
                  fontFamily: 'inherit',
                  color: 'text.secondary',
                  opacity: 0.5,
                  minWidth: 20,
                  userSelect: 'none',
                }}
              >
                {String(i + 1).padStart(3, '0')}
              </Typography>
              <Typography
                variant="caption"
                sx={{
                  fontFamily: 'inherit',
                  color: levelColors[entry.level],
                  fontWeight: entry.level === 'error' ? 700 : 400,
                  minWidth: 40,
                }}
              >
                {entry.level.toUpperCase()}
              </Typography>
              <Typography
                variant="caption"
                sx={{
                  fontFamily: 'inherit',
                  color: 'text.secondary',
                  opacity: 0.5,
                  minWidth: 80,
                }}
              >
                {new Date(entry.timestamp).toLocaleTimeString()}
              </Typography>
              <Typography
                variant="caption"
                sx={{
                  fontFamily: 'inherit',
                  color: '#e0e0e0',
                  wordBreak: 'break-word',
                }}
              >
                {entry.message}
              </Typography>
            </Box>
          ))
        )}
        <div ref={bottomRef} />
      </Box>
    </Box>
  );
}
