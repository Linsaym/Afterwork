import { useCallback, useMemo, useState } from 'react'
import { Activity, BarChart3, CalendarDays, Check, ChevronLeft, ChevronRight, Clock3, Gamepad2, History, Trophy, X } from 'lucide-react'
import { AchievementPreview } from './achievements'
import { Dialog } from './dialog'
import { desktop } from './desktop'
import { sessionName } from './games'
import { HistoryNavigation, useHistoryQuery } from './history'
import { dateKey, dateOf, duration, recordedSeconds, stopwatch, type Game, type GameTotals, type Snapshot } from './types'

const weekdays = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс']
const monthFormat = new Intl.DateTimeFormat('ru-RU', { month: 'long', year: 'numeric' })
const dayFormat = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short' })
const timeFormat = new Intl.DateTimeFormat('ru-RU', { hour: '2-digit', minute: '2-digit' })
const completionDateFormat = new Intl.DateTimeFormat('ru-RU', { dateStyle: 'long' })

type GoalCompletion = { sessionId: number; time: number; names: string[] }
function CompletedGoals({ activity, id, count, revision, close }: { activity: string; id: number; count: number; revision: number; close: () => void }) {
  const [page, setPage] = useState(0)
  const query = useCallback(() => desktop.focusPage(id, page * 24, 24), [id, page])
  const history = useHistoryQuery(query, revision)
  const days = new Map<string, { date: Date; completions: GoalCompletion[] }>()
  for (const session of history?.value?.sessions ?? []) {
    const completion = { sessionId: session.id, time: session.finished ?? session.spans.reduce((last, [, end]) => Math.max(last, end), session.started), names: session.completedGoals ?? [] }
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
      {!history?.value && <p className="empty-inline">{history?.error ?? 'Загрузка выполненных целей…'}</p>}
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
    <HistoryNavigation page={page} more={!!history?.value?.more} change={setPage} />
    <footer className="goal-history-footer">Выполнено целей: <strong>{count}</strong></footer>
  </Dialog>
}

function EditDuration({ seconds, save, close }: { seconds: number; save: (seconds: number) => Promise<unknown>; close: () => void }) {
  const [value, setValue] = useState(() => stopwatch(seconds))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const parts = /^(\d+):([0-5]\d):([0-5]\d)$/.exec(value.trim())
  const next = parts ? Number(parts[1]) * 3600 + Number(parts[2]) * 60 + Number(parts[3]) : NaN
  const invalid = !Number.isSafeInteger(next) || next > seconds
  const hint = Number.isNaN(next) ? 'Введите время в формате чч:мм:сс.'
    : invalid ? 'Длительность можно только уменьшить.' : `До ${stopwatch(seconds)} · чч:мм:сс`

  return <Dialog label="Длительность сессии" className="duration-dialog" close={() => { if (!saving) close() }}>
    <header className="dialog-heading">
      <span className="card-icon"><Clock3 size={19} aria-hidden="true" /></span>
      <h2>Длительность сессии</h2>
    </header>
    <form onSubmit={async event => {
      event.preventDefault()
      if (saving || invalid || next === seconds) return
      setSaving(true)
      setError('')
      try { await save(next); close() }
      catch (cause) { setError(String(cause)); setSaving(false) }
    }}>
      <input className="duration-input" type="text" aria-label="Длительность в формате часы, минуты, секунды"
        aria-describedby="duration-hint" aria-invalid={invalid} value={value} readOnly={saving}
        onChange={event => setValue(event.target.value)} onFocus={event => event.currentTarget.select()} autoFocus />
      <p id="duration-hint" className={invalid ? 'error' : undefined}>{hint}</p>
      {error && <p className="error" role="alert">{error}</p>}
      <div className="dialog-actions">
        <button type="button" className="button secondary" disabled={saving} onClick={close}>Отмена</button>
        <button type="submit" className="button primary" disabled={saving || invalid || next === seconds}>Сохранить</button>
      </div>
    </form>
  </Dialog>
}

export function Statistics({ data, openAchievements, shortenSession }: {
  data: Snapshot; openAchievements: () => void; shortenSession: (id: number, seconds: number) => Promise<unknown>
}) {
  const today = dateOf(data.date)
  const [offset, setOffset] = useState(0)
  const [page, setPage] = useState(0)
  const [goalActivity, setGoalActivity] = useState<number>()
  const [editingId, setEditingId] = useState<number>()
  const report = data.report
  const days = report?.totals.days ?? {}
  const week = Array.from({ length: 7 }, (_, index) => new Date(today.getFullYear(), today.getMonth(), today.getDate() + index - 6, 12))
  const weekValues = week.map(date => days[dateKey(date)] ?? 0)
  const weekTotal = weekValues.reduce((sum, value) => sum + value, 0)
  const max = Math.max(data.target * 2, ...weekValues)
  const month = new Date(today.getFullYear(), today.getMonth() + offset, 1, 12)
  const empty = (month.getDay() + 6) % 7
  const monthLength = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate()
  const activities = data.activities.map(item => ({
    ...item, seconds: report?.totals.activities[item.id] ?? 0, goalsDone: report?.goals[item.id] ?? 0,
  })).filter(item => item.seconds > 0 || item.goalsDone).sort((a, b) => b.seconds - a.seconds)
  const selectedActivity = activities.find(item => item.id === goalActivity)
  const total = activities.reduce((sum, item) => sum + item.seconds, 0)
  const query = useCallback(() => desktop.focusPage(null, page * 8, 8), [page])
  const result = useHistoryQuery(query, report ? data.workRevision : null)
  const history = result?.value?.sessions ?? []
  const activitiesById = useMemo(() => new Map(data.activities.map(item => [item.id, item.name])), [data.activities])
  const editing = history.find(session => session.id === editingId)
  return <div className="page">
    <div className="heading"><div><h1>Статистика</h1><p>Время по занятиям, календарь активности и история сессий.</p></div></div>
    {!report ? <div className="loading">Загрузка статистики…</div> : <>
      <div className="stats-grid">
        <section className="panel list-panel weekly-panel">
          <div className="card-heading"><span className="card-icon"><BarChart3 size={19} /></span><div><h2>Последние 7 дней</h2><p>{duration(weekTotal)} работы за неделю</p></div></div>
          {weekTotal > 0 ? <div className="week-chart" role="list" aria-label="Время работы по дням">
            {week.map((date, index) => <div className="bar-column" role="listitem" aria-label={dayFormat.format(date) + ' · ' + duration(weekValues[index])} key={dateKey(date)} title={dayFormat.format(date) + ' · ' + duration(weekValues[index])}>
              <small className="week-value">{weekValues[index] > 0 ? duration(weekValues[index]) : '—'}</small>
              <div className="bar-well"><div className={index === 6 ? 'bar today' : 'bar'} style={{ height: (weekValues[index] ? Math.max(2, weekValues[index] / max * 100) : 0) + '%' }} /></div>
              <small>{weekdays[(date.getDay() + 6) % 7]}</small>
            </div>)}
          </div> : <p className="chart-empty">За последние 7 дней нет записанного времени.</p>}
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
          <div className="calendar-legend"><span>Нет времени</span><i /><i className="some" /><i className="half" /><i className="full" /><span>Дневная цель</span></div>
        </section>
        <section className="panel list-panel">
          <div className="card-heading"><span className="card-icon"><Activity size={19} /></span><div><h2>Время по занятиям</h2><p>{duration(total)} за всё время</p></div></div>
          {activities.length ? <ul className="activity-distribution">{activities.map(item => <li key={item.id}>
            <div className="activity-distribution-row">
              <b>{item.name}</b>
              <span className="activity-distribution-duration">{duration(item.seconds)}</span>
              {item.goalsDone ? <button type="button" className="goal-history-metric" aria-haspopup="dialog"
                aria-label={`Выполнено целей: ${item.goalsDone}. Занятие «${item.name}»`}
                title="Посмотреть выполненные цели" onClick={() => setGoalActivity(item.id)}>
                <Check size={13} aria-hidden="true" /><span>Цели: {item.goalsDone}</span>
              </button> : <span className="goal-history-metric" title="Выполнено целей: 0" aria-label="Выполнено целей: 0">
                <Check size={13} aria-hidden="true" /><span>Цели: 0</span>
              </span>}
            </div>
            <div className="progress-track"><span style={{ width: item.seconds / Math.max(1, total) * 100 + '%' }} /></div>
          </li>)}</ul> : <p className="empty-inline">Здесь появится время ваших занятий.</p>}
        </section>
        <AchievementPreview awards={data.awards} open={openAchievements} />
      </div>
      <GameStatistics revision={data.gameRevision} games={data.games} />
      <section className="panel list-panel history">
        <div className="card-heading"><span className="card-icon"><History size={19} /></span><div><h2>История сессий</h2><p>Время и результаты завершённых сессий</p></div></div>
        {!history.length ? <p className="empty-inline">{result?.error ?? (result?.value ? 'Завершённые сессии появятся здесь.' : 'Загрузка истории…')}</p> : <div className="history-list">{history.map(session => {
          const date = new Date(session.started * 1000)
          return <div className="history-row" key={session.id}><div><b>{activitiesById.get(session.activity) ?? 'Занятие'}</b><small>{dayFormat.format(date)} · {timeFormat.format(date)}</small>{!!session.completedGoals?.length && <div className="goal-results"><span className="goal-badge"><Trophy size={16} />Цели достигнуты</span><ul>{session.completedGoals.map(g => <li key={g}>{g}</li>)}</ul></div>}{session.note && <p>{session.note}</p>}</div><button type="button" className="history-duration" aria-haspopup="dialog"
            aria-label={`Изменить длительность сессии · ${duration(recordedSeconds(session))}`}
            disabled={recordedSeconds(session) === 0} onClick={() => setEditingId(session.id)}>{duration(recordedSeconds(session))}</button></div>
        })}</div>}
        <HistoryNavigation page={page} more={!!result?.value?.more} change={setPage} />
      </section>
    </>}
    {!!selectedActivity?.goalsDone && <CompletedGoals activity={selectedActivity.name} id={selectedActivity.id} count={selectedActivity.goalsDone} revision={data.workRevision} close={() => setGoalActivity(undefined)} />}
    {editing && <EditDuration seconds={recordedSeconds(editing)} close={() => setEditingId(undefined)}
      save={seconds => shortenSession(editing.id, seconds)} />}
  </div>
}

const gameChartPreference = 'afterwork.hideGameCharts'
const timeOfDay = ['Ночь · 00:00–06:00', 'Утро · 06:00–12:00', 'День · 12:00–18:00', 'Вечер · 18:00–00:00']

function summarizeGames(totals: Record<string, GameTotals>, games: Game[]) {
  const secondsByTime = timeOfDay.map(() => 0)
  const gamesByPath = new Map(games.flatMap(game => [game.path, ...game.aliases].map(path => [path, game] as const)))
  const playTimes = new Map<string, { name: string; seconds: number }>()
  for (const [path, { seconds, buckets }] of Object.entries(totals)) {
    if (seconds <= 0) continue
    const game = gamesByPath.get(path)
    const key = game?.path ?? path
    const entry = playTimes.get(key) ?? { name: game?.name ?? sessionName(path), seconds: 0 }
    entry.seconds += seconds
    playTimes.set(key, entry)

    buckets.forEach((seconds, index) => { secondsByTime[index] += seconds })
  }
  const topGames = [...playTimes].sort(([, a], [, b]) =>
    b.seconds - a.seconds || a.name.localeCompare(b.name, 'ru'),
  ).slice(0, 5)
  return { secondsByTime, topGames }
}

function GameStatistics({ revision, games }: { revision: number | null; games: Game[] }) {
  const [hidden, setHidden] = useState(() => localStorage.getItem(gameChartPreference) === 'true')
  const toggle = () => {
    localStorage.setItem(gameChartPreference, String(!hidden))
    setHidden(!hidden)
  }
  const query = useCallback(() => desktop.gameReport(null, [], 0, 1), [])
  const report = useHistoryQuery(query, hidden ? null : revision)
  const totals = report?.value?.totals
  const summary = useMemo(() => totals ? summarizeGames(totals, games) : null, [totals, games])
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
      {!summary ? <p className="empty-inline">{report?.error ?? 'Загрузка игровой истории…'}</p> : !summary.topGames.length ? <p className="empty-inline">Диаграммы появятся после первой игровой сессии.</p> :
        <div className="stats-grid game-statistics-charts">
          <div>
            <h3>Игровое время по времени суток</h3>
            <div className="distribution">{timeOfDay.map((label, index) => <div key={label}>
              <div><b>{label}</b><span>{duration(summary.secondsByTime[index])}</span></div>
              <div className="progress-track" aria-hidden="true"><span style={{ width: summary.secondsByTime[index] / longest * 100 + '%' }} /></div>
            </div>)}</div>
          </div>
          <div>
            <h3>Время по играм</h3>
            <div className="distribution">{summary.topGames.map(([key, game]) => <div key={key}>
              <div><b>{game.name}</b><span>{duration(game.seconds)}</span></div>
              <div className="progress-track" aria-hidden="true"><span style={{ width: game.seconds / summary.topGames[0][1].seconds * 100 + '%' }} /></div>
            </div>)}</div>
          </div>
        </div>}
    </div>
  </section>
}
