import { useEffect, useMemo, useRef, useState } from 'react'
import { desktop } from './desktop'
import { BarChart3, Check, EyeOff, Gamepad2, History, ImagePlus, Plus, ScanLine, Search, ShieldCheck, Trash2 } from 'lucide-react'
import { duration, type Game, type GameIconStatus, type GameSession, type Snapshot } from './types'

const periods = [{ key: 'day', label: 'День' }, { key: 'week', label: 'Неделя' }, { key: 'month', label: 'Месяц' }, { key: 'all', label: 'Всё время' }] as const
type Period = typeof periods[number]['key']
const dayFormat = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short' })
const timeFormat = new Intl.DateTimeFormat('ru-RU', { hour: '2-digit', minute: '2-digit' })
const dateTime = new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium', timeStyle: 'short' })
export const sessionName = (path: string) => path.replace(/^.*[\\/]/, '').replace(/\.exe$/i, '')
const pathLabel = (path: string) => path.startsWith('\\\\?\\') ? path.slice(4) : path
const emptyHistory: GameSession[] = []

function midnight(day: Date, offset = 0, hour = 0) {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate() + offset, hour).getTime() / 1000
}

function chartRanges(period: Period, today: Date, earliest: number) {
  if (period === 'day') return Array.from({ length: 8 }, (_, i) => {
    const start = midnight(today, 0, i * 3)
    return { start, end: midnight(today, 0, (i + 1) * 3), label: timeFormat.format(start * 1000) }
  })

  const from = new Date(earliest * 1000)
  const calendar = (day: Date) => Date.UTC(day.getFullYear(), day.getMonth(), day.getDate())
  const days = period === 'week' ? 7 : period === 'month' ? 30 : Math.max(1, (calendar(today) - calendar(from)) / 86400000 + 1)
  const step = period === 'month' ? 3 : Math.max(1, Math.ceil(days / 12))
  return Array.from({ length: Math.ceil(days / step) }, (_, i) => {
    const offset = 1 - days + i * step
    const start = midnight(today, offset)
    return { start, end: midnight(today, Math.min(1, offset + step)), label: dayFormat.format(start * 1000) }
  })
}

const overlap = (session: GameSession, start: number, end: number) => Math.max(0, Math.min(session.end, end) - Math.max(session.start, start))

export function GameIcon({ path, status, large = false }: { path: string; status?: GameIconStatus; large?: boolean }) {
  const revision = status?.revision
  const custom = status?.custom
  const [asset, setAsset] = useState<{ revision: string; url: string }>()
  useEffect(() => {
    if (!revision || custom === undefined) return
    let disposed = false
    let url: string | undefined
    void desktop.gameIcon(path, { revision, custom }).then(bytes => {
      if (disposed || !bytes.byteLength) return
      url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))
      setAsset({ revision, url })
    }).catch(() => {})
    return () => {
      disposed = true
      if (url) URL.revokeObjectURL(url)
    }
  }, [path, revision, custom])
  const url = asset?.revision === revision ? asset?.url : undefined
  return <span className={'tracked-icon' + (large ? ' large' : '')} aria-hidden="true">
    {url ? <img src={url} alt="" draggable={false} onError={() => setAsset(undefined)} /> : <Gamepad2 size={large ? 28 : 19} />}
  </span>
}

export function Games({ data, busy, add, remove, exclude, editIcon }: { data: Snapshot; busy: boolean; add: () => void; remove: (game: Game) => void; exclude: (game: Game) => void; editIcon: (game: Game) => void }) {
  const [period, setPeriod] = useState<Period>('week')
  const [selected, setSelected] = useState<string>()
  const [showExcluded, setShowExcluded] = useState(false)
  const [query, setQuery] = useState('')
  const chart = useRef<HTMLElement>(null)
  const [historyLimit, setHistoryLimit] = useState(8)
  const history = data.gameHistory ?? emptyHistory
  const selectedGame = data.games.find(game => game.path === selected)
  const filtered = selectedGame?.path
  const today = new Date()
  const date = midnight(today)
  const report = useMemo(() => {
    const totals = new Map<string, { seconds: number; latest: number }>()
    let earliest = date
    for (const session of history) {
      earliest = Math.min(earliest, session.start)
      const total = totals.get(session.game) ?? { seconds: 0, latest: 0 }
      total.seconds += session.end - session.start
      total.latest = Math.max(total.latest, session.end)
      totals.set(session.game, total)
    }
    const ranges = chartRanges(period, new Date(date * 1000), earliest)
    const relevant = history.filter(session => (!filtered || session.game === filtered) && overlap(session, ranges[0].start, ranges.at(-1)!.end) > 0)
    const buckets = ranges.map(range => ({ ...range, seconds: relevant.reduce((sum, session) => sum + overlap(session, range.start, range.end), 0) }))
    return { totals, buckets, sessions: relevant.sort((a, b) => b.end - a.end), seconds: buckets.reduce((sum, item) => sum + item.seconds, 0) }
  }, [history, period, filtered, date])
  const max = Math.max(1, ...report.buckets.map(item => item.seconds))
  const excludedCount = data.games.filter(game => game.excluded).length
  const games = data.games.filter(game => showExcluded === game.excluded)
    .sort((a, b) => (report.totals.get(b.path)?.latest ?? 0) - (report.totals.get(a.path)?.latest ?? 0) || a.name.localeCompare(b.name, 'ru'))
  const search = query.trim().toLocaleLowerCase('ru')
  const visibleGames = games.filter(game => game.name.toLocaleLowerCase('ru').includes(search))

  useEffect(() => { setHistoryLimit(8) }, [period, filtered])

  return <div className="page game-page">
    <div className="heading"><div><h1>Игры</h1><p>Распознанные игры, время и история сессий.</p></div><button className="button secondary" onClick={add} disabled={busy}><Plus size={16} />Добавить вручную</button></div>
    <section className="panel list-panel game-report" ref={chart} aria-label="Игровое время" tabIndex={-1}>
      <div className="game-report-head">
        <div className="card-heading">{selectedGame ? <GameIcon path={selectedGame.path} status={data.gameIcons?.[selectedGame.path]} /> : <span className="card-icon"><BarChart3 size={19} /></span>}<div><h2>Игровое время</h2><p title={selectedGame?.name}>{selectedGame?.name ?? 'Все игры'}</p></div></div>
        <div className="activity-tabs" role="group" aria-label="Период статистики">{periods.map(item => <button type="button" key={item.key} aria-pressed={period === item.key} onClick={() => setPeriod(item.key)}>{item.label}</button>)}</div>
      </div>
      <div className="game-report-number"><strong>{duration(report.seconds)}</strong>{selectedGame && <button className="button secondary" onClick={() => setSelected(undefined)}>Показать все игры</button>}</div>
      {!data.gameHistory ? <p className="empty-inline">Загрузка игрового времени…</p> : report.seconds === 0 ? <p className="chart-empty">За выбранный период нет игрового времени.</p> : <div className="game-chart" role="list" aria-label="Игровое время по периодам">
        {report.buckets.map(({ start, label, seconds }) => <div className="game-bar-column" role="listitem" key={start} aria-label={label + ': ' + duration(seconds)} title={label + ' · ' + duration(seconds)}>
          <div className="game-bar-track"><span style={{ height: seconds ? Math.max(3, (seconds / max) * 100) + '%' : '0%' }} /></div><small>{label}</small>
        </div>)}
      </div>}
    </section>
    <section className="panel list-panel detected-games">
      <div className="game-report-head">
        <div className="card-heading"><span className="card-icon"><Gamepad2 size={19} /></span><div><h2>Мои игры</h2><p>{data.games.length - excludedCount} отслеживается · {data.blockedCount} заблокировано</p></div></div>
        <div className="activity-tabs" role="group" aria-label="Список игр"><button type="button" aria-pressed={!showExcluded} onClick={() => { setShowExcluded(false); setQuery('') }}>Активные</button><button type="button" aria-pressed={showExcluded} onClick={() => { setShowExcluded(true); setQuery('') }}>Исключения <small>{excludedCount}</small></button></div>
      </div>
      {games.length > 0 && <label className="activity-search game-search"><Search size={16} /><input aria-label="Поиск игр" placeholder="Поиск по названию…" value={query} onChange={event => setQuery(event.target.value)} /></label>}
      {visibleGames.length ? <div className="tracked-list">{visibleGames.map(game => {
        const summary = report.totals.get(game.path)
        const playing = data.runningGames.includes(game.path)
        const failure = [game.path, ...game.aliases].map(path => data.failures[path]).find(Boolean)
        const blocked = !game.excluded && data.locked
        return <div className="tracked-row" key={game.path}>
          <button className="tracked-name" aria-pressed={filtered === game.path} onClick={() => {
            setSelected(filtered === game.path ? undefined : game.path)
            chart.current?.focus({ preventScroll: true })
            chart.current?.scrollIntoView({ block: 'start' })
          }} title={'Показать статистику игры · ' + pathLabel(game.path)}>
            <GameIcon path={game.path} status={data.gameIcons?.[game.path]} /><span><b title={game.name}>{game.name}</b><small>{game.source || 'Вручную'}{playing && <em>Сейчас в игре</em>}</small></span>
          </button>
          <div className="tracked-meta"><b>{duration(summary?.seconds ?? 0)}</b><small>{playing ? 'Сессия идёт' : summary ? dateTime.format(summary.latest * 1000) : 'Сессий пока нет'}</small></div>
          <span className={'game-lock-label' + (failure ? ' error' : '')}>
            {game.excluded ? 'Исключена' : blocked ? failure ? 'Блокировка неполная' : 'Заблокирована' : 'Доступна'}
            {failure && <small>{failure}</small>}
          </span>
          <div className="tracked-actions">
            <button className="icon-button" disabled={busy} title="Изменить иконку" aria-label={'Изменить иконку «' + game.name + '»'} onClick={() => editIcon(game)}><ImagePlus size={16} /></button>
            <button className="button secondary" disabled={busy} aria-label={(game.excluded ? 'Вернуть ' : 'Исключить ') + game.name} title={game.excluded ? 'Вернуть учёт и блокировку' : 'Отключить учёт и блокировку'} onClick={() => exclude(game)}>{game.excluded ? <Check size={14} /> : <EyeOff size={14} />}<span>{game.excluded ? 'Вернуть' : 'Исключить'}</span></button>
            {!game.catalog_id && <button className="icon-button" disabled={busy} title="Удалить из списка" aria-label={'Удалить ' + game.name} onClick={() => remove(game)}><Trash2 size={15} /></button>}
          </div>
        </div>
      })}</div> : <div className="empty"><ScanLine size={27} /><h3>{search ? 'Игры не найдены' : showExcluded ? 'Исключений пока нет' : 'Пока нет распознанных игр'}</h3><p>{search ? 'Попробуйте другое название.' : showExcluded ? 'Исключённые игры перестают учитываться и блокироваться.' : 'Запустите игру. Afterwork добавит её в список, если распознает.'}</p></div>}
    </section>
    <section className="panel list-panel game-session-list">
      <div className="card-heading"><span className="card-icon"><History size={19} /></span><div><h2>История игр</h2><p>Сессии за выбранный период</p></div></div>
      {report.sessions.length ? <div className="history-list">{report.sessions.slice(0, historyLimit).map(session => <div className="history-row" key={session.id}>
        <div className="game-history-label"><GameIcon path={session.game} status={data.gameIcons?.[session.game]} /><div><b>{data.games.find(game => game.path === session.game)?.name ?? sessionName(session.game)}</b><small>{dateTime.format(session.start * 1000)}</small></div></div><span>{duration(session.end - session.start)}</span>
      </div>)}</div> : <p className="empty-inline">{data.gameHistory ? 'За этот период нет игровых сессий.' : 'Загрузка истории…'}</p>}
      {report.sessions.length > historyLimit && <button className="button secondary" onClick={() => setHistoryLimit(historyLimit + 8)}>Ещё сессии</button>}
    </section>
    <div className="info-line"><ShieldCheck size={17} />Статистика хранится на этом компьютере. Блокировка применяется к новым запускам.</div>
  </div>
}
