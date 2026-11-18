import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import type { Action, GameIconStatus, GameReport, HistoryPage, Session, Snapshot } from './types'

export type SnapshotOptions = { report: boolean; games: boolean; achievements: boolean }
export type UpdateStatus = {
  currentVersion: string
  autoUpdate: boolean
  phase: 'disabled' | 'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'ready' | 'installing'
  version: string | null
  received: number
  total: number | null
  error: string | null
}

export const desktop = {
  snapshot: (options: SnapshotOptions) => invoke<Snapshot>('snapshot', options),
  focusPage: (activity: number | null, offset: number, limit: number) => invoke<HistoryPage<Session>>('focus_page', { activity, offset, limit }),
  gameReport: (filtered: string | null, ranges: [number, number][], offset: number, limit: number) => invoke<GameReport>('game_report', { filtered, ranges, offset, limit }),
  apply: (action: Action) => invoke<void>('apply', { action }),
  gameIcon: async (game: string, status: GameIconStatus) => {
    const bytes = await invoke<ArrayBuffer | number[]>('game_icon', { game, ...status })
    return Array.isArray(bytes) ? Uint8Array.from(bytes).buffer : bytes
  },
  setGameIcon: (game: string, file: string | null) => invoke<void>('set_game_icon', { game, file }),
  autostart: (enabled?: boolean) => invoke<boolean>('autostart', { enabled }),
  updateStatus: () => invoke<UpdateStatus>('update_status'),
  setAutoUpdate: (enabled: boolean) => invoke<void>('set_auto_update', { enabled }),
  checkUpdate: () => invoke<void>('check_update'),
  downloadUpdate: () => invoke<void>('download_update'),
  installUpdate: (version: string) => invoke<void>('install_update', { version }),
  chooseGame: () => invoke<string | null>('choose_game'),
  chooseIcon: () => open({
    title: 'Выберите иконку игры', directory: false, multiple: false,
    filters: [{ name: 'Изображения', extensions: ['png', 'jpg', 'jpeg', 'ico'] }],
  }),
  window: getCurrentWindow,
}
