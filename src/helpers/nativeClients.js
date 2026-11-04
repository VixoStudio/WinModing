import { invoke } from '@tauri-apps/api/core'

export const getNativeClientRelease = (client = 'win') => invoke('native_client_latest', { client })
export const installNativeClient = (client = 'win') => invoke('install_native_client', { client })
