import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import type { Action, GameIconStatus, Snapshot } from './types'

export type SnapshotOptions = { report: boolean; games: boolean; achievements: boolean }

export const desktop = {
  snapshot: (options: SnapshotOptions) => invoke<Snapshot>('snapshot', options),
  apply: (action: Action) => invoke<void>('apply', { action }),
  gameIcon: (game: string, status: GameIconStatus) => invoke<ArrayBuffer>('game_icon', { game, ...status }),
  setGameIcon: (game: string, file: string | null) => invoke<void>('set_game_icon', { game, file }),
  autostart: (enabled?: boolean) => invoke<boolean>('autostart', { enabled }),
  chooseGame: () => open({
    title: 'Выберите файл игры', directory: false, multiple: false,
    filters: [{ name: 'Исполняемые файлы', extensions: ['exe'] }],
  }),
  chooseIcon: () => open({
    title: 'Выберите иконку игры', directory: false, multiple: false,
    filters: [{ name: 'Изображения', extensions: ['png', 'jpg', 'jpeg', 'ico'] }],
  }),
  window: getCurrentWindow,
}
