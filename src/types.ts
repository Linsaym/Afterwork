export type ActivityIcon = 'sparkles' | 'briefcase' | 'book' | 'code' | 'pencil' | 'palette' | 'music' | 'dumbbell' | 'languages' | 'brain' | 'target' | 'heart'
export type ActivityColor = 'violet' | 'blue' | 'cyan' | 'green' | 'amber' | 'orange' | 'rose' | 'slate'
export type ActivityDetails = { goalsEnabled?: boolean; goals?: string[]; name: string; icon: ActivityIcon; color: ActivityColor }
export type Activity = ActivityDetails & { id: number; archived: boolean }
export type ActivitySummaries = Record<number, { seconds: number; lastActive: number }>
export type Game = { path: string; name: string; source: string; catalog_id: string; aliases: string[]; excluded: boolean; shortcut?: string }
export type GameSession = { id: number; game: string; start: number; end: number }
export type GameIconStatus = { revision: string; custom: boolean }
export type Session = { id: number; activity: number; started: number; running: number | null; spans: [number, number][]; note: string; finished?: number | null; completedGoals?: string[] }
export type Settings = { first: number; second: number; minutes: number; notifications: boolean }
export type Report = { totals: { days: Record<string, number>; activities: Record<string, number> }; goals: Record<number, number> }
export type HistoryPage<T> = { sessions: T[]; more: boolean }
export type GameTotals = { count: number; seconds: number; latest: number; buckets: number[] }
export type GameReport = HistoryPage<GameSession> & { totals: Record<string, GameTotals>; buckets: number[] }
export type Snapshot = {
  date: string; seconds: number; phase: number; target: number; locked: boolean; settings: Settings
  activities: Activity[]; activitySummaries: ActivitySummaries; games: Game[]; active: Session | null; elapsed: number
  failures: Record<string, string>; blockedCount: number; report: Report | null
  runningGames: string[]; gameRevision: number | null; gameEarliest: number | null; workRevision: number
  gameIcons: Record<string, GameIconStatus> | null
  awards: Record<string, number>
  achievementProgress: Record<string, { current: number; target: number }> | null
}
export type Action =
  | { type: 'start'; activity: number }
  | { type: 'pause' | 'resume' }
  | { type: 'finish'; note: string; completedGoals?: string[] }
  | { type: 'shortenSession'; id: number; seconds: number }
  | { type: 'addActivity'; details: ActivityDetails }
  | { type: 'editActivity'; id: number; details: ActivityDetails }
  | { type: 'archiveActivity'; id: number; archived: boolean }
  | { type: 'addGame' | 'removeGame'; path: string }
  | { type: 'excludeGame'; path: string; excluded: boolean }
  | { type: 'saveSettings'; settings: Settings }

export const pad = (value: number) => String(value).padStart(2, '0')
export const clock = (minutes: number) => pad(Math.floor(minutes / 60)) + ':' + pad(minutes % 60)
export const dateOf = (key: string) => new Date(key + 'T12:00:00')
export const dateKey = (date: Date) => date.getFullYear() + '-' + pad(date.getMonth() + 1) + '-' + pad(date.getDate())
export const duration = (seconds: number) => {
  if (seconds > 0 && seconds < 60) return seconds + ' сек'
  const minutes = Math.floor(seconds / 60)
  return minutes >= 60 ? Math.floor(minutes / 60) + ' ч ' + pad(minutes % 60) + ' мин' : minutes + ' мин'
}
export const stopwatch = (seconds: number) => [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60].map(pad).join(':')
export const recordedSeconds = (session: Session) => session.spans.reduce((sum, [start, end]) => sum + end - start, 0)
