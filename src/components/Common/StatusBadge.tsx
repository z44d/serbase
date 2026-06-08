import Chip from '@mui/material/Chip';
import type { DBStatus } from '../../database/types';

const statusConfig: Record<DBStatus, { color: 'success' | 'warning' | 'error' | 'default'; label: string }> = {
  running: { color: 'success', label: 'Running' },
  starting: { color: 'warning', label: 'Starting' },
  stopped: { color: 'default', label: 'Stopped' },
  error: { color: 'error', label: 'Error' },
};

interface Props {
  status: DBStatus;
  size?: 'small' | 'medium';
}

export function StatusBadge({ status, size = 'small' }: Props) {
  const config = statusConfig[status];
  return (
    <Chip
      label={config.label}
      color={config.color}
      size={size}
      variant="outlined"
      sx={{
        fontWeight: 600,
        fontSize: size === 'small' ? '0.7rem' : '0.8rem',
        height: size === 'small' ? 22 : 28,
      }}
    />
  );
}
