import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const listLocalServers = () => invoke('list_local_servers')
export const saveLocalServer = (server) => invoke('save_local_server', { server })
export const deleteLocalServer = (id) => invoke('delete_local_server', { id })
export const installLocalServer = (server) => invoke('install_local_server', { server })
export const startLocalServer = (server) => invoke('start_local_server', { server })
export const stopLocalServer = (id) => invoke('stop_local_server', { id })
export const localServerRunning = (id) => invoke('local_server_running', { id })
export const onLocalServerLog = (callback) => listen('local-server-log', (event) => callback(event.payload))
