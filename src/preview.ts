import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { canBlockGame, dateKey, minecraftJava, priorityGoalLimit, recordedSeconds, samePriorityGoal, type Action, type GameReport, type GameSession, type HistoryPage, type PriorityGoal, type Session, type Snapshot } from './types'
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
    priorityGoals: [], activitySummaries: {}, games: [], active: null, elapsed: 0,
    failures: {}, blockedCount: 0, report: null, runningGames: [], gameRevision: null, gameEarliest: null, workRevision: 0,
    gameIcons: null, awards: {}, achievementProgress: null,
  }
  if (scenario === 'priorities') {
    data.activities[0].goalsEnabled = true
    data.activities[0].goals = ['Собрать первый прототип', 'Набросать план презентации']
    data.activities[1].goalsEnabled = true
    data.activities[1].goals = ['Дочитать главу о привычках', 'Выписать три идеи из книги']
    data.activities.push({ id: 3, name: 'Английский', icon: 'languages', color: 'blue', archived: false,
      goalsEnabled: true, goals: ['Подготовить вопросы к разговорному клубу'] })
    data.priorityGoals = [
      { activity: 1, goal: 'Собрать первый прототип' },
      { activity: 3, goal: 'Подготовить вопросы к разговорному клубу' },
      { activity: 2, goal: 'Дочитать главу о привычках' },
    ]
  }
  if (scenario === 'demo' || scenario === 'archive') {
    data.activities.push({ id: 3, name: 'Старый проект', icon: 'briefcase', color: 'blue', archived: true })
    history.push({ id: 4, activity: 3, started: started - 1800, running: null, spans: [[started - 1800, started]], note: 'Пример завершённой сессии' })
  }
  if (scenario === 'demo' || scenario === 'locked' || scenario === 'error') {
    data.games.push({ path: gamePath, name: 'Тестовая игра', source: 'Steam', catalog_id: 'steam:1', aliases: [], excluded: false })
  }
  if (scenario === 'locked') {
    data.games.push({ path: minecraftJava, name: 'Minecraft: Java Edition', source: 'Распознано по окну', catalog_id: minecraftJava, aliases: [], excluded: false })
  }
  if (scenario === 'demo' || scenario === 'active' || scenario === 'update') {
    data.active = { id: 5, activity: 1, started, running: started, spans: [], note: '' }
  }
  if (scenario === 'error') data.failures[gamePath] = 'Файл занят другой программой. Afterwork повторит попытку.'
  const gameHistory: GameSession[] = []
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

  const hasActiveGoal = (goal: PriorityGoal) => data.activities.some(item =>
    item.id === goal.activity && !item.archived && item.goalsEnabled && item.goals?.includes(goal.goal))
  const retainPriorities = () => { data.priorityGoals = data.priorityGoals.filter(hasActiveGoal) }

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
        retainPriorities()
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
        if (action.type === 'editActivity' && action.goalRenames?.length) {
          const renames = action.goalRenames
          data.priorityGoals = data.priorityGoals.map(goal => {
            if (goal.activity !== action.id) return goal
            const rename = renames.find(item => item.from === goal.goal)
            return { ...goal, goal: rename?.to ?? (renames.some(item => item.to === goal.goal) ? '' : goal.goal) }
          })
        }
        retainPriorities()
        break
      case 'setPriorityGoals':
        if (action.goals.length > priorityGoalLimit) throw new Error('Можно выбрать до трёх ближайших целей.')
        if (action.goals.some((goal, index) => !hasActiveGoal(goal) || action.goals.slice(0, index).some(item => samePriorityGoal(item, goal)))) {
          throw new Error('Список целей изменился. Проверьте выбранные цели.')
        }
        data.priorityGoals = action.goals
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
          blockedCount: locked && scenario !== 'error' ? data.games.filter(game => !game.excluded && canBlockGame(game)).length : 0,
          report: payload?.report ? { totals: {
            days: seconds ? { [data.date]: seconds } : {},
            activities: Object.fromEntries(Object.entries(summaries).map(([id, item]) => [id, item.seconds])),
          }, goals: history.reduce<Record<number, number>>((goals, session) => {
            if (session.completedGoals?.length) goals[session.activity] = (goals[session.activity] ?? 0) + session.completedGoals.length
            return goals
          }, {}) } : null,
          achievementProgress: payload?.achievements ? achievementProgress : null,
          gameRevision: payload?.games || payload?.report ? 0 : null,
          gameEarliest: payload?.games && gameHistory.length ? Math.min(...gameHistory.map(item => item.start)) : null,
        } satisfies Snapshot)
      }
      case 'focus_page': {
        const activity = payload?.activity as number | null
        const offset = Number(payload?.offset)
        const limit = Number(payload?.limit)
        const time = (session: Session) => session.finished ?? session.spans.reduce((last, [, end]) => Math.max(last, end), session.started)
        const rows = activity === null ? [...history].reverse() : history
          .filter(session => session.activity === activity && session.completedGoals?.length)
          .sort((a, b) => time(b) - time(a) || b.id - a.id)
        return structuredClone({ sessions: rows.slice(offset, offset + limit), more: rows.length > offset + limit } satisfies HistoryPage<Session>)
      }
      case 'game_report': {
        const filtered = payload?.filtered as string | null
        const ranges = payload?.ranges as [number, number][]
        const offset = Number(payload?.offset)
        const limit = Number(payload?.limit)
        const totals: GameReport['totals'] = {}
        for (const session of gameHistory) {
          const entry = totals[session.game] ??= { count: 0, seconds: 0, latest: 0, buckets: [0, 0, 0, 0] }
          entry.count++
          entry.seconds += session.end - session.start
          entry.latest = Math.max(entry.latest, session.end)
          for (let cursor = session.start; cursor < session.end;) {
            const date = new Date(cursor * 1000)
            const bucket = Math.floor(date.getHours() / 6)
            const end = Math.min(session.end, date.setHours((bucket + 1) * 6, 0, 0, 0) / 1000)
            if (end <= cursor) break
            entry.buckets[bucket] += end - cursor
            cursor = end
          }
        }
        const overlap = (session: GameSession, start: number, end: number) => Math.max(0, Math.min(session.end, end) - Math.max(session.start, start))
        const rows = ranges.length ? gameHistory.filter(session => (!filtered || session.game === filtered)
          && overlap(session, ranges[0][0], ranges.at(-1)![1]) > 0).sort((a, b) => b.end - a.end) : []
        return structuredClone({ totals, buckets: ranges.map(([start, end]) => rows.reduce((sum, row) => sum + overlap(row, start, end), 0)),
          sessions: rows.slice(offset, offset + limit), more: rows.length > offset + limit } satisfies GameReport)
      }
      case 'apply': {
        apply(payload?.action as Action)
        data.workRevision++
        return
      }
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
      case 'choose_game':
      case 'plugin:dialog|open': return null
      case 'plugin:window|minimize':
      case 'plugin:window|toggle_maximize':
      case 'plugin:window|close': return undefined
      default: throw new Error(`Команда не настроена в превью: ${command}`)
    }
  })
}
