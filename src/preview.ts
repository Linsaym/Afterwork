import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { dateKey, recordedSeconds, type Action, type Session, type Snapshot } from './types'
import type { UpdateStatus } from './desktop'
import updateFixture from '../fixtures/ipc-update-status.json'
import { version } from '../package.json'

const gamePath = 'C:/Demo/Game.exe'
const now = () => Math.floor(Date.now() / 1000)

export function installPreview() {
  const scenario = new URLSearchParams(location.search).get('scenario') ?? 'demo'
  const started = now() - 900
  let nextId = 10
  let autostart = false
  const updatePreview = scenario === 'update' || scenario === 'update-offline'
  const updates: UpdateStatus = {
    ...updateFixture, currentVersion: version, phase: updatePreview ? 'checking' : 'disabled', version: null, received: 0, total: null,
  }
  function download() {
    if (updates.phase !== 'available') return
    updates.phase = 'downloading'
    updates.total = updateFixture.total
    const progress = window.setInterval(() => {
      updates.received += 128
      if (updates.received === updates.total) {
        window.clearInterval(progress)
        updates.phase = 'ready'
      }
    }, 300)
  }
  function check() {
    updates.phase = 'checking'
    updates.error = null
    window.setTimeout(() => {
      if (scenario === 'update-offline') {
        updates.phase = 'idle'
        updates.error = 'Не удалось проверить обновления. Проверьте подключение к интернету и доступность GitHub Releases.'
      } else {
        updates.phase = 'available'
        const [major, minor, patch] = version.split('.').map(Number)
        updates.version = `${major}.${minor}.${patch + 1}`
        if (updates.autoUpdate) download()
      }
    }, 1200)
  }
  if (updatePreview) check()
  const history: Session[] = []
  const data: Snapshot = {
    date: dateKey(new Date()), seconds: 0, phase: 1, target: 2700, locked: true,
    settings: { first: 360, second: 960, minutes: 45, notifications: true },
    activities: [
      { id: 1, name: 'Личный проект', icon: 'target', color: 'violet', archived: false },
      { id: 2, name: 'Чтение', icon: 'book', color: 'green', archived: false },
    ],
    activitySummaries: {}, games: [], active: null, elapsed: 0,
    failures: {}, blockedCount: 0, report: null, runningGames: [], gameHistory: null,
    gameIcons: null, awards: {}, achievementProgress: null,
  }
  if (scenario === 'demo' || scenario === 'archive') {
    data.activities.push({ id: 3, name: 'Старый проект', icon: 'briefcase', color: 'blue', archived: true })
    history.push({ id: 4, activity: 3, started: started - 1800, running: null, spans: [[started - 1800, started]], note: 'Пример завершённой сессии' })
  }
  if (scenario === 'demo' || scenario === 'locked' || scenario === 'error') {
    data.games.push({ path: gamePath, name: 'Тестовая игра', source: 'Steam', catalog_id: 'steam:1', aliases: [], excluded: false })
  }
  if (scenario === 'demo' || scenario === 'active' || scenario === 'update') {
    data.active = { id: 5, activity: 1, started, running: started, spans: [], note: '' }
  }
  if (scenario === 'error') data.failures[gamePath] = 'Файл занят другой программой. Afterwork повторит попытку.'
  const gameHistory: NonNullable<Snapshot['gameHistory']> = []
  if (scenario === 'game-stats') {
    const firstGame = 'C:/Demo/StardewValley.exe'
    const secondGame = 'C:/Demo/Hades.exe'
    data.games.push(
      { path: firstGame, name: 'Stardew Valley', source: 'Steam', catalog_id: '', aliases: [], excluded: false },
      { path: secondGame, name: 'Hades', source: 'Steam', catalog_id: '', aliases: [], excluded: false },
    )
    const time = (day: number, hour: number, minute = 0) => {
      const date = new Date()
      date.setDate(date.getDate() + day)
      date.setHours(hour, minute, 0, 0)
      return date.getTime() / 1000
    }
    gameHistory.push(
      { id: 1, game: firstGame, start: time(-3, 17, 30), end: time(-3, 20) },
      { id: 2, game: firstGame, start: time(-2, 23, 30), end: time(-1, 1) },
      { id: 3, game: secondGame, start: time(-1, 11), end: time(-1, 13) },
    )
  }

  function newAchievementProgress(time: number) {
    const completed = history.filter(session => session.completedGoals?.length)
    const counts = completed.map(session => session.completedGoals?.length ?? 0)
    const total = counts.reduce((sum, count) => sum + count, 0)
    const distinct = new Set(completed.map(session => session.activity)).size
    const games = new Set<string>()
    const days = new Map<string, [number, number][]>()
    let longest = 0
    for (const session of gameHistory) {
      const end = Math.min(session.end, time)
      longest = Math.max(longest, end - session.start)
      if (end - session.start >= 600) games.add(session.game)
      for (let from = session.start; from < end;) {
        const day = new Date(from * 1000)
        if (day.getHours() < 6) day.setDate(day.getDate() - 1)
        const key = dateKey(day)
        const next = new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1, 6).getTime() / 1000
        const to = Math.min(next, end)
        const spans = days.get(key) ?? []
        spans.push([from, to])
        days.set(key, spans)
        from = to
      }
    }
    const playedDays = [...days.values()].filter(spans => {
      let elapsed = 0
      let previousEnd = -Infinity
      for (const [start, end] of spans.sort(([a], [b]) => a - b)) {
        elapsed += Math.max(0, end - Math.max(start, previousEnd))
        previousEnd = Math.max(previousEnd, end)
      }
      return elapsed >= 600
    }).length
    const progress = (current: number, target: number) => ({ current: Math.min(current, target), target })
    return {
      'activity-goal-first': progress(total, 1),
      'activity-goal-ten': progress(total, 10),
      'activity-goal-triple': progress(counts.reduce((max, count) => Math.max(max, count), 0), 3),
      'activity-goal-variety': progress(distinct, 3),
      'game-first-minute': progress(longest, 60),
      'game-variety': progress(games.size, 3),
      'game-days': progress(playedDays, 3),
    }
  }

  function pause(session: Session, time: number) {
    if (session.running === null) return
    session.spans.push([session.running, time])
    session.running = null
  }

  function apply(action: Action) {
    const time = now()
    const session = data.active
    switch (action.type) {
      case 'start':
        if (session) throw new Error('Завершите текущую сессию.')
        data.active = { id: nextId++, activity: action.activity, started: time, running: time, spans: [], note: '' }
        break
      case 'pause':
        if (session) pause(session, time)
        break
      case 'resume':
        if (session) session.running ??= time
        break
      case 'finish':
        if (!session) break
        pause(session, time)
        session.completedGoals = [...(action.completedGoals ?? [])]
        const activity = data.activities.find(a => a.id === session.activity)
        if (activity) activity.goals = activity.goals?.filter(g => !session.completedGoals!.includes(g))
        history.push({ ...session, finished: time, note: action.note })
        data.active = null
        break
      case 'shortenSession': {
        const completed = history.find(item => item.id === action.id && item.running === null)
        if (!completed) throw new Error('Завершённая сессия не найдена.')
        if (!Number.isSafeInteger(action.seconds) || action.seconds < 0 || action.seconds > recordedSeconds(completed)) {
          throw new Error('Длительность можно только уменьшить.')
        }
        let remaining = action.seconds
        completed.spans = completed.spans.flatMap(([start, end]) => {
          const kept = Math.min(remaining, end - start)
          remaining -= kept
          return kept > 0 ? [[start, start + kept]] : []
        })
        break
      }
      case 'addActivity':
        data.activities.push({ ...action.details, id: nextId++, archived: false })
        break
      case 'editActivity':
      case 'archiveActivity':
        data.activities = data.activities.map(item => item.id === action.id
          ? { ...item, ...(action.type === 'editActivity' ? action.details : { archived: action.archived }) }
          : item)
        break
      case 'saveSettings':
        data.settings = action.settings
        break
      case 'excludeGame':
        data.games = data.games.map(game => game.path === action.path ? { ...game, excluded: action.excluded } : game)
        break
      case 'addGame':
      case 'removeGame':
        throw new Error('Выбор файлов доступен в Windows-приложении.')
    }
  }

  mockWindows('main')
  mockIPC((command, args) => {
    const payload = args as Record<string, unknown> | undefined
    switch (command) {
      case 'snapshot': {
        const summaries: Snapshot['activitySummaries'] = {}
        const time = now()
        const achievementProgress = newAchievementProgress(time)
        for (const [id, { current, target }] of Object.entries(achievementProgress)) {
          if (current >= target) data.awards[id] ??= time
        }
        const elapsed = (session: Session) => recordedSeconds(session)
          + (session.running === null ? 0 : time - session.running)
        for (const session of data.active ? [...history, data.active] : history) {
          const summary = summaries[session.activity] ??= { seconds: 0, lastActive: 0 }
          summary.seconds += elapsed(session)
          summary.lastActive = Math.max(summary.lastActive, session.started)
        }
        const seconds = Object.values(summaries).reduce((sum, item) => sum + item.seconds, 0)
        const target = data.settings.minutes * 60
        const locked = seconds < target
        return structuredClone({
          ...data, seconds, target, locked, activitySummaries: summaries,
          elapsed: data.active ? elapsed(data.active) : 0,
          blockedCount: locked && scenario !== 'error' ? data.games.filter(game => !game.excluded).length : 0,
          report: payload?.report ? { totals: {
            days: seconds ? { [data.date]: seconds } : {},
            activities: Object.fromEntries(Object.entries(summaries).map(([id, item]) => [id, item.seconds])),
          }, history } : null,
          achievementProgress: payload?.achievements ? achievementProgress : null,
          gameHistory: payload?.games || payload?.report ? gameHistory : null,
        } satisfies Snapshot)
      }
      case 'apply': return apply(payload?.action as Action)
      case 'update_status': return structuredClone(updates)
      case 'set_auto_update':
        updates.autoUpdate = !!payload?.enabled
        if (updates.autoUpdate) download()
        return
      case 'check_update': return check()
      case 'download_update': return download()
      case 'install_update':
        updates.phase = 'installing'
        return
      case 'autostart':
        if (typeof payload?.enabled === 'boolean') autostart = payload.enabled
        return autostart
      case 'plugin:dialog|open': return null
      case 'plugin:window|minimize':
      case 'plugin:window|toggle_maximize':
      case 'plugin:window|close': return undefined
      default: throw new Error(`Команда не настроена в превью: ${command}`)
    }
  })
}
