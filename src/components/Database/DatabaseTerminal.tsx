import { useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import TextField from '@mui/material/TextField';
import IconButton from '@mui/material/IconButton';
import Typography from '@mui/material/Typography';
import SendIcon from '@mui/icons-material/Send';
import { useDatabaseStore } from '../../store/database-store';
import type { InstanceState, QueryResult } from '../../database/types';

interface TerminalEntry {
  type: 'input' | 'output' | 'error';
  text: string;
}

interface Props {
  instance: InstanceState;
}

export function DatabaseTerminal({ instance }: Props) {
  const [entries, setEntries] = useState<TerminalEntry[]>([
    { type: 'output', text: `serbase ${instance.type} terminal ready (${instance.host}:${instance.port})` },
    { type: 'output', text: `Type commands and press Enter or click Send.` },
  ]);
  const [input, setInput] = useState('');
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const executeQuery = useDatabaseStore((s) => s.executeQuery);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [entries]);

  useEffect(() => {
    inputRef.current?.focus();
  }, [instance.id]);

  const handleSend = async () => {
    const cmd = input.trim();
    if (!cmd) return;
    if (instance.status !== 'running') {
      setEntries((prev) => [
        ...prev,
        { type: 'input', text: `> ${cmd}` },
        { type: 'error', text: 'Database is not running. Start it first.' },
      ]);
      setInput('');
      return;
    }

    setEntries((prev) => [...prev, { type: 'input', text: `> ${cmd}` }]);
    setInput('');

    try {
      const res = await executeQuery(instance.id, cmd);
      const parsed: QueryResult = JSON.parse(res);

      if (parsed.rows.length === 0) {
        setEntries((prev) => [...prev, { type: 'output', text: '(no rows returned)' }]);
      } else {
        const output = parsed.rows
          .map((row) =>
            parsed.columns.length === 1
              ? String(Object.values(row)[0] ?? 'NULL')
              : JSON.stringify(row),
          )
          .join('\n');
        setEntries((prev) => [...prev, { type: 'output', text: output }]);
      }
    } catch (err) {
      setEntries((prev) => [...prev, { type: 'error', text: `Error: ${err}` }]);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleSend();
    }
  };

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
          TERMINAL
        </Typography>
      </Box>

      <Box
        sx={{
          flex: 1,
          overflow: 'auto',
          p: 1.5,
          bgcolor: '#060d17',
          fontFamily: '"JetBrains Mono", "Fira Code", monospace',
          fontSize: '0.8rem',
          lineHeight: 1.6,
        }}
      >
        {entries.map((entry, i) => (
          <Box
            key={i}
            sx={{
              color:
                entry.type === 'error'
                  ? '#f44336'
                  : entry.type === 'input'
                    ? '#80cbc4'
                    : '#e0e0e0',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}
          >
            {entry.text}
          </Box>
        ))}
        <div ref={bottomRef} />
      </Box>

      <Box
        sx={{
          display: 'flex',
          gap: 1,
          p: 1,
          borderTop: '1px solid',
          borderColor: 'divider',
          bgcolor: '#0a1929',
        }}
      >
        <TextField
          inputRef={inputRef}
          size="small"
          fullWidth
          placeholder="Enter command..."
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={instance.status !== 'running'}
          sx={{
            '& .MuiInputBase-root': {
              fontFamily: '"JetBrains Mono", monospace',
              fontSize: '0.8rem',
              bgcolor: '#060d17',
            },
          }}
        />
        <IconButton
          size="small"
          color="primary"
          onClick={handleSend}
          disabled={instance.status !== 'running' || !input.trim()}
        >
          <SendIcon fontSize="small" />
        </IconButton>
      </Box>
    </Box>
  );
}
