import { useState, useCallback } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import TextField from '@mui/material/TextField';
import Button from '@mui/material/Button';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import TableRowsIcon from '@mui/icons-material/TableRows';
import { useDatabaseStore } from '../../store/database-store';
import type { InstanceState, QueryResult } from '../../database/types';

interface Props {
  instance: InstanceState;
}

export function DataBrowser({ instance }: Props) {
  const [query, setQuery] = useState('');
  const [result, setResult] = useState<{ columns: string[]; rows: Record<string, unknown>[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const executeQuery = useDatabaseStore((s) => s.executeQuery);

  const handleExecute = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const res = await executeQuery(instance.id, query);
      const parsed: QueryResult = JSON.parse(res);
      setResult({ columns: parsed.columns, rows: parsed.rows });
    } catch (err) {
      setError(String(err));
      setResult(null);
    } finally {
      setLoading(false);
    }
  }, [query, instance.id, executeQuery]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && e.metaKey) {
      handleExecute();
    }
  };

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, height: '100%' }}>
      <Box sx={{ display: 'flex', gap: 1, alignItems: 'flex-start' }}>
        <TextField
          size="small"
          fullWidth
          placeholder={
            instance.type === 'postgres'
              ? 'SELECT * FROM sqlite_master LIMIT 10'
              : instance.type === 'redis'
                ? 'KEYS *'
                : '{"find": "collection", "filter": {}}'
          }
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          sx={{
            '& .MuiInputBase-root': {
              fontFamily: '"JetBrains Mono", monospace',
              fontSize: '0.8125rem',
            },
          }}
        />
        <Button
          variant="contained"
          size="small"
          startIcon={<PlayArrowIcon />}
          onClick={handleExecute}
          disabled={loading || instance.status !== 'running'}
        >
          Run
        </Button>
      </Box>

      {error && (
        <Typography variant="body2" color="error" sx={{ fontFamily: 'monospace', fontSize: '0.8rem' }}>
          {error}
        </Typography>
      )}

      {result && (
        <Box sx={{ flex: 1, overflow: 'auto' }}>
          <Box
            sx={{
              display: 'grid',
              gap: 0,
              border: '1px solid',
              borderColor: 'divider',
              borderRadius: 1,
              overflow: 'hidden',
            }}
          >
            <Box
              sx={{
                display: 'grid',
                gridTemplateColumns: `repeat(${result.columns.length}, minmax(120px, 1fr))`,
                bgcolor: 'rgba(255,255,255,0.03)',
                borderBottom: '1px solid',
                borderColor: 'divider',
              }}
            >
              {result.columns.map((col) => (
                <Box
                  key={col}
                  sx={{
                    px: 1.5,
                    py: 1,
                    fontSize: '0.75rem',
                    fontWeight: 700,
                    color: 'primary.main',
                    textTransform: 'uppercase',
                    letterSpacing: '0.05em',
                    fontFamily: '"JetBrains Mono", monospace',
                    borderRight: '1px solid',
                    borderColor: 'divider',
                    '&:last-child': { borderRight: 'none' },
                  }}
                >
                  {col}
                </Box>
              ))}
            </Box>

            {result.rows.length === 0 ? (
              <Box sx={{ p: 2, textAlign: 'center', color: 'text.secondary' }}>
                <TableRowsIcon sx={{ fontSize: 32, opacity: 0.3, mb: 1 }} />
                <Typography variant="body2">No rows returned</Typography>
              </Box>
            ) : (
              result.rows.map((row, i) => (
                <Box
                  key={i}
                  sx={{
                    display: 'grid',
                    gridTemplateColumns: `repeat(${result.columns.length}, minmax(120px, 1fr))`,
                    borderBottom: i < result.rows.length - 1 ? '1px solid' : 'none',
                    borderColor: 'divider',
                    bgcolor: i % 2 === 0 ? 'transparent' : 'rgba(255,255,255,0.015)',
                  }}
                >
                  {result.columns.map((col) => (
                    <Box
                      key={col}
                      sx={{
                        px: 1.5,
                        py: 0.75,
                        fontSize: '0.8rem',
                        fontFamily: '"JetBrains Mono", monospace',
                        borderRight: '1px solid',
                        borderColor: 'divider',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                        '&:last-child': { borderRight: 'none' },
                      }}
                    >
                      {String(row[col] ?? 'NULL')}
                    </Box>
                  ))}
                </Box>
              ))
            )}
          </Box>

          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ mt: 1, display: 'block', fontFamily: '"JetBrains Mono", monospace' }}
          >
            {result.rows.length} row(s) returned
          </Typography>
        </Box>
      )}
    </Box>
  );
}
