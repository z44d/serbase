import { useState } from 'react';
import Box from '@mui/material/Box';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemButton from '@mui/material/ListItemButton';
import ListItemIcon from '@mui/material/ListItemIcon';
import ListItemText from '@mui/material/ListItemText';
import ListSubheader from '@mui/material/ListSubheader';
import Typography from '@mui/material/Typography';
import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import TextField from '@mui/material/TextField';
import MenuItem from '@mui/material/MenuItem';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import AddIcon from '@mui/icons-material/Add';
import DeleteIcon from '@mui/icons-material/Delete';
import StorageIcon from '@mui/icons-material/Storage';
import MemoryIcon from '@mui/icons-material/Memory';
import HubIcon from '@mui/icons-material/Hub';
import { useDatabaseStore } from '../../store/database-store';
import { StatusBadge } from '../Common/StatusBadge';
import type { DBType } from '../../database/types';

const iconMap: Record<DBType, typeof StorageIcon> = {
  postgres: StorageIcon,
  redis: MemoryIcon,
  mongo: HubIcon,
};

export function DatabaseSidebar() {
  const instances = useDatabaseStore((s) => s.instances);
  const activeInstanceId = useDatabaseStore((s) => s.activeInstanceId);
  const setActiveInstance = useDatabaseStore((s) => s.setActiveInstance);
  const createServer = useDatabaseStore((s) => s.createServer);
  const removeServer = useDatabaseStore((s) => s.removeServer);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [newType, setNewType] = useState<DBType>('redis');
  const [newName, setNewName] = useState('');
  const [newHost, setNewHost] = useState('127.0.0.1');
  const [newPort, setNewPort] = useState('6379');
  const [newUsername, setNewUsername] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [newDatabase, setNewDatabase] = useState('');

  const defaultPorts: Record<DBType, string> = {
    postgres: '5432',
    redis: '6379',
    mongo: '27017',
  };

  const handleTypeChange = (type: DBType) => {
    setNewType(type);
    setNewPort(defaultPorts[type]);
  };

  const handleCreate = async () => {
    const port = parseInt(newPort, 10);
    if (isNaN(port) || port < 1 || port > 65535) return;

    await createServer({
      type: newType,
      name: newName,
      host: newHost,
      port,
      username: newUsername,
      password: newPassword,
      database: newDatabase,
    });

    setDialogOpen(false);
    setNewName('');
    setNewUsername('');
    setNewPassword('');
    setNewDatabase('');
  };

  const instanceList = Array.from(instances.values());

  return (
    <Box sx={{ py: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
      <Box sx={{ px: 2, mb: 1, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <Typography variant="caption" fontWeight={700} color="text.secondary" sx={{ letterSpacing: '0.05em' }}>
          SERVERS
        </Typography>
        <Tooltip title="Add new server">
          <IconButton size="small" onClick={() => setDialogOpen(true)} sx={{ color: 'primary.main' }}>
            <AddIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </Box>

      <List dense disablePadding sx={{ flex: 1, overflow: 'auto' }}>
        {instanceList.length === 0 && (
          <Box sx={{ px: 2, py: 4, textAlign: 'center' }}>
            <Typography variant="caption" color="text.secondary">
              No servers yet. Click + to add one.
            </Typography>
          </Box>
        )}
        {instanceList.map((instance) => {
          const Icon = iconMap[instance.type];
          const isActive = instance.id === activeInstanceId;

          return (
            <ListItem
              key={instance.id}
              disablePadding
              sx={{ px: 1 }}
              secondaryAction={
                <Tooltip title="Remove server">
                  <IconButton
                    edge="end"
                    size="small"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeServer(instance.id);
                    }}
                    sx={{ color: 'text.disabled', '&:hover': { color: 'error.main' } }}
                  >
                    <DeleteIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
              }
            >
              <ListItemButton
                selected={isActive}
                onClick={() => setActiveInstance(instance.id)}
                sx={{
                  borderRadius: 2,
                  mb: 0.25,
                  flexDirection: 'column',
                  alignItems: 'flex-start',
                  gap: 0.5,
                  py: 1.25,
                  pr: 5,
                  '&.Mui-selected': {
                    bgcolor: 'rgba(144, 202, 249, 0.08)',
                  },
                }}
              >
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, width: '100%' }}>
                  <ListItemIcon sx={{ minWidth: 32, color: isActive ? 'primary.main' : 'text.secondary' }}>
                    <Icon sx={{ fontSize: 20 }} />
                  </ListItemIcon>
                  <ListItemText
                    primary={instance.name || instance.label}
                    primaryTypographyProps={{
                      variant: 'body2',
                      fontWeight: 600,
                      sx: { color: isActive ? 'primary.main' : 'text.primary', overflow: 'hidden', textOverflow: 'ellipsis' },
                    }}
                    sx={{ my: 0 }}
                  />
                  <StatusBadge status={instance.status} />
                </Box>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, pl: 5.5 }}>
                  <Typography variant="caption" color="text.secondary">
                    {instance.host}:{instance.port}
                  </Typography>
                </Box>
              </ListItemButton>
            </ListItem>
          );
        })}
      </List>

      <Dialog open={dialogOpen} onClose={() => setDialogOpen(false)} maxWidth="xs" fullWidth>
        <DialogTitle>Add New Server</DialogTitle>
        <DialogContent>
          <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2, pt: 1 }}>
            <TextField
              select
              label="Type"
              size="small"
              value={newType}
              onChange={(e) => handleTypeChange(e.target.value as DBType)}
            >
              <MenuItem value="postgres">PostgreSQL</MenuItem>
              <MenuItem value="redis">Redis</MenuItem>
              <MenuItem value="mongo">MongoDB</MenuItem>
            </TextField>
            <TextField
              label="Server Name (optional)"
              size="small"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={`My ${newType}`}
            />
            <TextField
              label="Host"
              size="small"
              value={newHost}
              onChange={(e) => setNewHost(e.target.value)}
              placeholder="127.0.0.1"
            />
            <TextField
              label="Port"
              size="small"
              value={newPort}
              onChange={(e) => setNewPort(e.target.value)}
              type="number"
            />
            <TextField
              label="Database (optional)"
              size="small"
              value={newDatabase}
              onChange={(e) => setNewDatabase(e.target.value)}
              placeholder={newType === 'postgres' ? 'Defaults to username' : 'serbase'}
              disabled={newType === 'redis'}
            />
            <TextField
              label="Username (optional)"
              size="small"
              value={newUsername}
              onChange={(e) => setNewUsername(e.target.value)}
              placeholder="admin"
            />
            <TextField
              label="Password (optional)"
              size="small"
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
            />
          </Box>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDialogOpen(false)}>Cancel</Button>
          <Button variant="contained" onClick={handleCreate}>Add Server</Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
}
