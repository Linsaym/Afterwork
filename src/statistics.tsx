import { useState } from 'react'
import { Activity, BarChart3, CalendarDays, Check, ChevronLeft, ChevronRight, Gamepad2, History, Trophy, X } from 'lucide-react'
import { AchievementPreview } from './achievements'
import { Dialog } from './dialog'
import { sessionName } from './games'
import { dateKey, dateOf, duration, type Game, type GameSession, type Snapshot } from './types'

const weekdays = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс']
const monthFormat = new Intl.DateTimeFormat('ru-RU', { month: 'long', year: 'numeric' })
const dayFormat = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short' })
const timeFormat = new Intl.DateTimeFormat('ru-RU', { hour: '2-digit', minute: '2-digit' })
const completionDateFormat = new Intl.DateTimeFormat('ru-RU', { dateStyle: 'long' })

type GoalCompletion = { sessionId: number; time: number; names: string[] }
type GoalHistory = { count: number; completions: GoalCompletion[] }

function CompletedGoals({ activity, history, close }: { activity: string; history: GoalHistory; close: () => void }) {
  const days = new Map<string, { date: Date; completions: GoalCompletion[] }>()
  for (const completion of [...history.completions].sort((a, b) => b.time - a.time || b.sessionId - a.sessionId)) {
    const date = new Date(completion.time * 1000)
    const key = dateKey(date)
    const day = days.get(key)
    if (day) day.completions.push(completion)
    else days.set(key, { date, completions: [completion] })
  }

  return <Dialog label={`Выполненные цели — ${activity}`} className="goal-history-dialog" close={close}>
    <header className="goal-history-header">
      <span className="card-icon goal-history-emblem"><Trophy size={20} aria-hidden="true" /></span>
      <div><h2>Выполненные цели</h2><p>{activity}</p></div>
      <button type="button" className="icon-button" aria-label="Закрыть историю целей" onClick={close}><X size={17} /></button>
    </header>
    <div className="goal-history-scroll" tabIndex={0} role="region" aria-label="Выполненные цели по датам">
      {[...days].map(([key, { date, completions }]) => <section className="goal-history-day" key={key}>
        <h3><CalendarDays size={14} aria-hidden="true" /><time dateTime={key}>{completionDateFormat.format(date)}</time></h3>
        <ul className="goal-history-entries">
          {completions.map(({ sessionId, time, names }) => <li className="goal-history-entry" key={sessionId}>
            <time dateTime={new Date(time * 1000).toISOString()}>{timeFormat.format(time * 1000)}</time>
            <ul className="goal-history-names">{names.map((name, index) => <li key={index}>
              <span className="goal-history-check"><Check size={13} aria-hidden="true" /></span>
              <span>{name}</span>
            </li>)}</ul>
          </li>)}
        </ul>
      </section>)}
    </div>
    <footer className="goal-history-footer">Выполнено целей: <strong>{history.count}</strong></footer>
  </Dialog>
}

export function Statistics({ data, openAchievements }: { data: Snapshot; openAchievements: () => void }) {
  const today = dateOf(data.date)
  const [offset, setOffset] = useState(0)
  const [limit, setLimit] = useState(8)
  const [goalActivity, setGoalActivity] = useState<number>()
  const report = data.report
  const days = report?.totals.days ?? {}
  const week = Array.from({ length: 7 }, (_, index) => new Date(today.getFullYear(), today.getMonth(), today.getDate() + index - 6, 12))
  const weekValues = week.map(date => days[dateKey(date)] ?? 0)
  const max = Math.max(data.target * 2, ...weekValues)
  const month = new Date(today.getFullYear(), today.getMonth() + offset, 1, 12)
  const empty = (month.getDay() + 6) % 7
  const monthLength = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate()
  const goalHistory = new Map<number, GoalHistory>()
  for (const session of report?.history ?? []) {
    if (!session.completedGoals?.length) continue
    const entry = goalHistory.get(session.activity) ?? { count: 0, completions: [] }
    const time = session.finished ?? session.spans.reduce((last, [, end]) => Math.max(last, end), session.started)
    entry.count += session.completedGoals.length
    entry.completions.push({ sessionId: session.id, time, names: session.completedGoals })
    goalHistory.set(session.activity, entry)
  }
  const activities = data.activities.map(item => ({
    ...item, seconds: report?.totals.activities[item.id] ?? 0, goalsDone: goalHistory.get(item.id),
  })).filter(item => item.seconds > 0 || item.goalsDone).sort((a, b) => b.seconds - a.seconds)
  const selectedActivity = activities.find(item => item.id === goalActivity)
  const total = activities.reduce((sum, item) => sum + item.seconds, 0)
  const history = report ? [...report.history].reverse() : []
  return <div className="page">
    <div className="heading"><div><h1>Статистика</h1><p>Ваше время, привычки и маленькие победы.</p></div></div>
    {!report ? <div className="loading">Загрузка статистики…</div> : <>
      <div className="stats-grid">
        <section className="panel list-panel">
          <div className="card-heading"><span className="card-icon"><BarChart3 size={19} /></span><div><h2>Последние 7 дней</h2><p>{duration(weekValues.reduce((sum, value) => sum + value, 0))} работы за неделю</p></div></div>
          <div className="week-chart" aria-label="Время работы по дням">
            {week.map((date, index) => <div className="bar-column" key={dateKey(date)} title={dayFormat.format(date) + ' · ' + duration(weekValues[index])}>
              <div className="bar-well"><div className={index === 6 ? 'bar today' : 'bar'} style={{ height: (weekValues[index] ? Math.max(2, weekValues[index] / max * 100) : 0) + '%' }} /></div>
              <small>{weekdays[(date.getDay() + 6) % 7]}</small>
            </div>)}
          </div>
        </section>
        <section className="panel list-panel">
          <div className="card-heading"><span className="card-icon"><CalendarDays size={19} /></span><div><h2>Календарь активности</h2><p className="month-label">{monthFormat.format(month)}</p></div><div className="calendar-nav"><button className="icon-button" aria-label="Предыдущий месяц" onClick={() => setOffset(offset - 1)}><ChevronLeft size={17} /></button><button className="icon-button" aria-label="Следующий месяц" disabled={offset === 0} onClick={() => setOffset(offset + 1)}><ChevronRight size={17} /></button></div></div>
          <div className="calendar">
            {weekdays.map(day => <small key={day}>{day}</small>)}
            {Array.from({ length: empty }, (_, index) => <span key={'e' + index} />)}
            {Array.from({ length: monthLength }, (_, index) => {
              const date = new Date(month.getFullYear(), month.getMonth(), index + 1, 12)
              const key = dateKey(date)
              const value = days[key] ?? 0
              const level = value >= data.target * 2 ? 'full' : value >= data.target ? 'half' : value > 0 ? 'some' : ''
              return <div className={'calendar-day ' + level + (key === data.date ? ' is-today' : '')} key={key} title={dayFormat.format(date) + ' · ' + duration(value)}>{date.getDate()}</div>
            })}
          </div>
        </section>
        <section className="panel list-panel">
          <div className="card-heading"><span className="card-icon"><Activity size={19} /></span><div><h2>Время по занятиям</h2><p>{duration(total)} за всё время</p></div></div>
          {activities.length ? <div className="distribution">{activities.map(item => <div key={item.id}>
            <div><b>{item.name}</b><span>{duration(item.seconds)}</span></div>
            <div className="progress-track"><span style={{ width: item.seconds / Math.max(1, total) * 100 + '%' }} /></div>
            {item.goalsDone ? <button type="button" className="goal-history-trigger" aria-haspopup="dialog"
              aria-label={`Выполнено целей: ${item.goalsDone.count}. Занятие «${item.name}»`}
              onClick={() => setGoalActivity(item.id)}>
              <Trophy size={14} aria-hidden="true" />Выполнено целей: {item.goalsDone.count}<ChevronRight size={14} aria-hidden="true" />
            </button> : item.goalsEnabled && <small className="goal-history-zero">Выполнено целей: 0</small>}
          </div>)}</div> : <p className="empty-inline">Здесь появится время ваших занятий.</p>}
        </section>
        <AchievementPreview awards={data.awards} open={openAchievements} />
      </div>
      <GameStatistics history={data.gameHistory} games={data.games} />
      <section className="panel list-panel history">
        <div className="card-heading"><span className="card-icon"><History size={19} /></span><div><h2>История сессий</h2><p>Время и результаты завершённых сессий</p></div></div>
        {!history.length ? <p className="empty-inline">Завершённые сессии появятся здесь.</p> : <div className="history-list">{history.slice(0, limit).map(session => {
          const date = new Date(session.started * 1000)
          return <div className="history-row" key={session.id}><div><b>{data.activities.find(item => item.id === session.activity)?.name ?? 'Занятие'}</b><small>{dayFormat.format(date)} · {timeFormat.format(date)}</small>{!!session.completedGoals?.length && <div className="goal-results"><span className="goal-badge"><Trophy size={16} />Цели достигнуты</span><ul>{session.completedGoals.map(g => <li key={g}>{g}</li>)}</ul></div>}{session.note && <p>{session.note}</p>}</div><span>{duration(session.spans.reduce((sum, [start, end]) => sum + end - start, 0))}</span></div>
        })}</div>}
        {history.length > limit && <button className="button secondary" onClick={() => setLimit(limit + 8)}>Ещё сессии</button>}
      </section>
    </>}
    {selectedActivity?.goalsDone && <CompletedGoals activity={selectedActivity.name} history={selectedActivity.goalsDone} close={() => setGoalActivity(undefined)} />}
  </div>
}

const gameChartPreference = 'afterwork.hideGameCharts'
const timeOfDay = ['Ночь · 00:00–06:00', 'Утро · 06:00–12:00', 'День · 12:00–18:00', 'Вечер · 18:00–00:00']

function summarizeGames(history: GameSession[], games: Game[]) {
  const secondsByTime = timeOfDay.map(() => 0)
  const gamesByPath = new Map(games.flatMap(game => [game.path, ...game.aliases].map(path => [path, game] as const)))
  const playCounts = new Map<string, { name: string; count: number; seconds: number }>()
  for (const { game: path, start, end } of history) {
    if (!Number.isFinite(new Date(start * 1000).getTime()) || !Number.isFinite(new Date(end * 1000).getTime()) || end <= start) continue
    const game = gamesByPath.get(path)
    const key = game?.path ?? path
    const entry = playCounts.get(key) ?? { name: game?.name ?? sessionName(path), count: 0, seconds: 0 }
    entry.count += 1
    entry.seconds += end - start
    playCounts.set(key, entry)

    // Split a session at local time boundaries, including midnight.
    for (let cursor = start; cursor < end;) {
      const local = new Date(cursor * 1000)
      const bucket = Math.floor(local.getHours() / 6)
      const boundary = new Date(local.getFullYear(), local.getMonth(), local.getDate(), (bucket + 1) * 6).getTime() / 1000
      if (!Number.isFinite(boundary) || boundary <= cursor) break
      const next = Math.min(end, boundary)
      secondsByTime[bucket] += next - cursor
      cursor = next
    }
  }
  const topGames = [...playCounts].sort(([, a], [, b]) =>
    b.count - a.count || b.seconds - a.seconds || a.name.localeCompare(b.name, 'ru'),
  ).slice(0, 5)
  return { secondsByTime, topGames }
}

function GameStatistics({ history, games }: { history: GameSession[] | null; games: Game[] }) {
  const [hidden, setHidden] = useState(() => localStorage.getItem(gameChartPreference) === 'true')
  const toggle = () => {
    localStorage.setItem(gameChartPreference, String(!hidden))
    setHidden(!hidden)
  }
  const summary = !hidden && history ? summarizeGames(history, games) : null
  const longest = Math.max(1, ...(summary?.secondsByTime ?? []))

  return <section className="panel list-panel history">
    <div className="card-heading game-statistics-heading">
      <span className="card-icon"><Gamepad2 size={19} /></span>
      <div><h2>Игровая статистика</h2><p>За всё время</p></div>
      <button type="button" className="button secondary" aria-expanded={!hidden} aria-controls="game-statistics-charts" onClick={toggle}>
        {hidden ? 'Показать диаграммы' : 'Скрыть диаграммы'}
      </button>
    </div>
    <div id="game-statistics-charts" hidden={hidden}>
      {!summary ? <p className="empty-inline">Загрузка игровой истории…</p> : !summary.topGames.length ? <p className="empty-inline">Диаграммы появятся после первой игровой сессии.</p> :
        <div className="stats-grid game-statistics-charts">
          <div>
            <h3>Игровое время по времени суток</h3>
            <div className="distribution">{timeOfDay.map((label, index) => <div key={label}>
              <div><b>{label}</b><span>{duration(summary.secondsByTime[index])}</span></div>
              <div className="progress-track" aria-hidden="true"><span style={{ width: summary.secondsByTime[index] / longest * 100 + '%' }} /></div>
            </div>)}</div>
          </div>
          <div>
            <h3>Игры по числу сессий</h3>
            <div className="distribution">{summary.topGames.map(([key, game]) => <div key={key}>
              <div><b>{game.name}</b><span>{game.count} · {duration(game.seconds)}</span></div>
              <div className="progress-track" aria-hidden="true"><span style={{ width: game.count / summary.topGames[0][1].count * 100 + '%' }} /></div>
            </div>)}</div>
          </div>
        </div>}
    </div>
  </section>
}
