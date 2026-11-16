import { useState } from 'react'
import { Activity, BarChart3, CalendarDays, ChevronLeft, ChevronRight, Gamepad2, History } from 'lucide-react'
import { AchievementPreview } from './achievements'
import { sessionName } from './games'
import { dateKey, dateOf, duration, type Game, type GameSession, type Snapshot } from './types'

const weekdays = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс']
const monthFormat = new Intl.DateTimeFormat('ru-RU', { month: 'long', year: 'numeric' })
const dayFormat = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short' })
const timeFormat = new Intl.DateTimeFormat('ru-RU', { hour: '2-digit', minute: '2-digit' })

export function Statistics({ data, openAchievements }: { data: Snapshot; openAchievements: () => void }) {
  const today = dateOf(data.date)
  const [offset, setOffset] = useState(0)
  const [limit, setLimit] = useState(8)
  const report = data.report
  const days = report?.totals.days ?? {}
  const week = Array.from({ length: 7 }, (_, index) => new Date(today.getFullYear(), today.getMonth(), today.getDate() + index - 6, 12))
  const weekValues = week.map(date => days[dateKey(date)] ?? 0)
  const max = Math.max(data.target * 2, ...weekValues)
  const month = new Date(today.getFullYear(), today.getMonth() + offset, 1, 12)
  const empty = (month.getDay() + 6) % 7
  const monthLength = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate()
  const activities = data.activities.map(item => ({ ...item, seconds: report?.totals.activities[item.id] ?? 0 })).filter(item => item.seconds > 0).sort((a, b) => b.seconds - a.seconds)
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
          {activities.length ? <div className="distribution">{activities.map(item => <div key={item.id}><div><b>{item.name}</b><span>{duration(item.seconds)}</span></div><div className="progress-track"><span style={{ width: item.seconds / total * 100 + '%' }} /></div></div>)}</div> : <p className="empty-inline">Здесь появится время ваших занятий.</p>}
        </section>
        <AchievementPreview awards={data.awards} open={openAchievements} />
      </div>
      <GameStatistics history={data.gameHistory} games={data.games} />
      <section className="panel list-panel history">
        <div className="card-heading"><span className="card-icon"><History size={19} /></span><div><h2>История сессий</h2><p>Время и результаты завершённых сессий</p></div></div>
        {!history.length ? <p className="empty-inline">Завершённые сессии появятся здесь.</p> : <div className="history-list">{history.slice(0, limit).map(session => {
          const date = new Date(session.started * 1000)
          return <div className="history-row" key={session.id}><div><b>{data.activities.find(item => item.id === session.activity)?.name ?? 'Занятие'}</b><small>{dayFormat.format(date)} · {timeFormat.format(date)}</small>{session.note && <p>{session.note}</p>}</div><span>{duration(session.spans.reduce((sum, [start, end]) => sum + end - start, 0))}</span></div>
        })}</div>}
        {history.length > limit && <button className="button secondary" onClick={() => setLimit(limit + 8)}>Ещё сессии</button>}
      </section>
    </>}
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
