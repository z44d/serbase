import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import StorageIcon from '@mui/icons-material/Storage';

export function EmptyState() {
  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100%',
        gap: 2,
        color: 'text.secondary',
      }}
    >
      <StorageIcon sx={{ fontSize: 64, opacity: 0.3 }} />
      <Typography variant="h6" color="text.secondary">
        Select a database instance
      </Typography>
      <Typography variant="body2" color="text.secondary" sx={{ opacity: 0.7 }}>
        Choose an instance from the sidebar or start a new one
      </Typography>
    </Box>
  );
}
