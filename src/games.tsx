import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'
import { desktop } from './desktop'
import { HistoryNavigation, useHistoryQuery } from './history'
import { BarChart3, Check, Ellipsis, EyeOff, Gamepad2, History, ImagePlus, Plus, ScanLine, Search, ShieldCheck, Trash2 } from 'lucide-react'
import { canBlockGame, duration, type Game, type GameIconStatus, type Snapshot } from './types'

const periods = [{ key: 'day', label: 'День' }, { key: 'week', label: 'Неделя' }, { key: 'month', label: 'Месяц' }, { key: 'all', label: 'Всё время' }] as const
type Period = typeof periods[number]['key']
const dayFormat = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short' })
const timeFormat = new Intl.DateTimeFormat('ru-RU', { hour: '2-digit', minute: '2-digit' })
const dateTime = new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium', timeStyle: 'short' })
export const sessionName = (path: string) => path.replace(/^.*[\\/]/, '').replace(/\.exe$/i, '')
const pathLabel = (path: string) => path.startsWith('\\\\?\\') ? path.slice(4) : path

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

function GameActions({ game, busy, statistics, editIcon, exclude, remove }: {
  game: Game; busy: boolean; statistics: () => void
  editIcon: (game: Game) => void; exclude: (game: Game) => void; remove: (game: Game) => void
}) {
  const id = useId()
  const trigger = useRef<HTMLButtonElement>(null)
  const popup = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)

  function run(action: () => void) {
    popup.current?.hidePopover()
    trigger.current?.focus({ preventScroll: true })
    action()
  }

  function position() {
    const button = trigger.current
    const menu = popup.current
    if (!button || !menu) return
    const { top, right, bottom } = button.getBoundingClientRect()
    const roomBelow = window.innerHeight - bottom
    const above = roomBelow < 180 && top > roomBelow
    menu.style.right = Math.max(8, window.innerWidth - right) + 'px'
    menu.style.top = above ? 'auto' : bottom + 6 + 'px'
    menu.style.bottom = above ? window.innerHeight - top + 6 + 'px' : 'auto'
    menu.style.maxHeight = Math.max(0, (above ? top : roomBelow) - 14) + 'px'
  }

  useEffect(() => {
    if (!busy) return
    popup.current?.hidePopover()
  }, [busy])

  useEffect(() => {
    if (!open) return
    window.addEventListener('resize', position)
    window.addEventListener('scroll', position, true)
    return () => {
      window.removeEventListener('resize', position)
      window.removeEventListener('scroll', position, true)
    }
  }, [open])

  return <div className="tracked-actions">
    <button ref={trigger} type="button" className="icon-button game-actions-button" popoverTarget={id}
      disabled={busy} aria-label={'Действия для игры «' + game.name + '»'} onClick={position}>
      <Ellipsis size={19} />
    </button>
    <div ref={popup} id={id} className="game-actions-popover" popover="auto" role="group" aria-label={'Действия для игры «' + game.name + '»'}
      onToggle={event => setOpen(event.newState === 'open')}
    >
      <button type="button" onClick={() => run(statistics)}><BarChart3 size={16} />Показать статистику</button>
      <button type="button" onClick={() => run(() => editIcon(game))}><ImagePlus size={16} />Изменить иконку</button>
      <button type="button" onClick={() => run(() => exclude(game))}>{game.excluded ? <Check size={16} /> : <EyeOff size={16} />}{game.excluded ? 'Вернуть в активные' : 'Исключить'}</button>
      {!game.catalog_id && <button type="button" className="game-actions-remove" onClick={() => run(() => remove(game))}><Trash2 size={16} />Удалить из списка</button>}
    </div>
  </div>
}

export function Games({ data, busy, add, remove, exclude, editIcon }: { data: Snapshot; busy: boolean; add: () => void; remove: (game: Game) => void; exclude: (game: Game) => void; editIcon: (game: Game) => void }) {
  const [period, setPeriod] = useState<Period>('week')
  const [selected, setSelected] = useState<string>()
  const [showExcluded, setShowExcluded] = useState(false)
  const [query, setQuery] = useState('')
  const chart = useRef<HTMLElement>(null)
  const [historyPage, setHistoryPage] = useState(0)
  const [catalogPage, setCatalogPage] = useState(0)
  const selectedGame = data.games.find(game => game.path === selected)
  const filtered = selectedGame?.path
  const today = new Date()
  const date = midnight(today)
  const earliest = period === 'all' ? Math.min(date, data.gameEarliest ?? date) : date
  const ranges = useMemo(() => chartRanges(period, new Date(date * 1000), earliest), [period, date, earliest])
  const queryReport = useCallback(() => desktop.gameReport(filtered ?? null, ranges.map(({ start, end }) => [start, end]), historyPage * 8, 8), [filtered, ranges, historyPage])
  const result = useHistoryQuery(queryReport, data.gameRevision)
  const report = result?.value
  const totals = report?.totals
  const seconds = report?.buckets.reduce((sum, item) => sum + item, 0) ?? 0
  const max = Math.max(1, ...report?.buckets ?? [])
  const excludedCount = data.games.filter(game => game.excluded).length
  const gamesByPath = useMemo(() => new Map(data.games.map(game => [game.path, game.name])), [data.games])
  const games = useMemo(() => data.games.filter(game => showExcluded === game.excluded)
    .sort((a, b) => (totals?.[b.path]?.latest ?? 0) - (totals?.[a.path]?.latest ?? 0) || a.name.localeCompare(b.name, 'ru')), [data.games, totals, showExcluded])
  const search = query.trim().toLocaleLowerCase('ru')
  const visibleGames = games.filter(game => game.name.toLocaleLowerCase('ru').includes(search))

  const visiblePage = Math.min(catalogPage, Math.max(0, Math.ceil(visibleGames.length / 50) - 1))

  return <div className="page game-page">
    <div className="heading"><div><h1>Игры</h1><p>Распознанные игры, время и история сессий.</p></div><button className="button secondary" onClick={add} disabled={busy}><Plus size={16} />Добавить вручную</button></div>
    <section className="panel list-panel game-report" ref={chart} aria-label="Игровое время" tabIndex={-1}>
      <div className="game-report-head">
        <div className="card-heading">{selectedGame ? <GameIcon path={selectedGame.path} status={data.gameIcons?.[selectedGame.path]} /> : <span className="card-icon"><BarChart3 size={19} /></span>}<div><h2>Игровое время</h2><p title={selectedGame?.name}>{selectedGame?.name ?? 'Все игры'}</p></div></div>
        <div className="activity-tabs" role="group" aria-label="Период статистики">{periods.map(item => <button type="button" key={item.key} aria-pressed={period === item.key} onClick={() => { setPeriod(item.key); setHistoryPage(0) }}>{item.label}</button>)}</div>
      </div>
      <div className="game-report-number"><strong>{duration(seconds)}</strong>{selectedGame && <button className="button secondary" onClick={() => { setSelected(undefined); setHistoryPage(0) }}>Показать все игры</button>}</div>
      {!report ? <p className="empty-inline">{result?.error ?? 'Загрузка игрового времени…'}</p> : seconds === 0 ? <p className="chart-empty">За выбранный период нет игрового времени.</p> : <div className="game-chart" role="list" aria-label="Игровое время по периодам">
        {ranges.map(({ start, label }, index) => {
          const seconds = report.buckets[index] ?? 0
          return <div className="game-bar-column" role="listitem" key={start} aria-label={label + ': ' + duration(seconds)} title={label + ' · ' + duration(seconds)}>
            <div className="game-bar-track"><span style={{ height: seconds ? Math.max(3, (seconds / max) * 100) + '%' : '0%' }} /></div><small>{label}</small>
          </div>
        })}
      </div>}
    </section>
    <section className="panel list-panel detected-games">
      <div className="game-report-head">
        <div className="card-heading"><span className="card-icon"><Gamepad2 size={19} /></span><div><h2>Мои игры</h2><p>{data.games.length - excludedCount} отслеживается{data.settings.gameBlocking && ` · ${data.blockedCount} заблокировано`}</p></div></div>
        <div className="activity-tabs" role="group" aria-label="Список игр"><button type="button" aria-pressed={!showExcluded} onClick={() => { setShowExcluded(false); setQuery(''); setCatalogPage(0) }}>Активные</button><button type="button" aria-pressed={showExcluded} onClick={() => { setShowExcluded(true); setQuery(''); setCatalogPage(0) }}>Исключения <small>{excludedCount}</small></button></div>
      </div>
      {games.length > 0 && <label className="activity-search game-search"><Search size={16} /><input aria-label="Поиск игр" placeholder="Поиск по названию…" value={query} onChange={event => { setQuery(event.target.value); setCatalogPage(0) }} /></label>}
      {visibleGames.length ? <div className="tracked-list">{visibleGames.slice(visiblePage * 50, (visiblePage + 1) * 50).map(game => {
        const summary = totals?.[game.path]
        const playing = data.runningGames.includes(game.path)
        const failure = [game.path, ...game.aliases].map(path => data.failures[path]).find(Boolean)
        const blocked = !game.excluded && data.locked
        const trackingOnly = !canBlockGame(game)
        const statistics = (path: string | undefined) => {
          setSelected(path)
          setHistoryPage(0)
          chart.current?.focus({ preventScroll: true })
          chart.current?.scrollIntoView({ block: 'start' })
        }
        return <div className="tracked-row" key={game.path}>
          <button className="tracked-name" aria-pressed={filtered === game.path} onClick={() => statistics(filtered === game.path ? undefined : game.path)} title={trackingOnly ? 'Показать статистику игры' : 'Показать статистику игры · ' + pathLabel(game.path)}>
            <GameIcon path={game.path} status={data.gameIcons?.[game.path]} /><span><b title={game.name}>{game.name}</b><small>{game.source || 'Вручную'}{playing && <em>Сейчас в игре</em>}</small></span>
          </button>
          <div className="tracked-meta"><b>{duration(summary?.seconds ?? 0)}</b><small>{playing ? 'Сессия идёт' : summary ? dateTime.format(summary.latest * 1000) : 'Сессий пока нет'}</small></div>
          <span className={'game-lock-label' + (failure ? ' error' : '')}>
            {game.excluded ? 'Исключена' : trackingOnly || !data.settings.gameBlocking ? 'Только учёт времени' : blocked ? failure ? 'Блокировка неполная' : 'Заблокирована' : 'Доступна'}
            {failure && <small>{failure}</small>}
          </span>
          <GameActions game={game} busy={busy} statistics={() => statistics(game.path)} editIcon={editIcon} exclude={exclude} remove={remove} />
        </div>
      })}</div> : <div className="empty"><ScanLine size={27} /><h3>{search ? 'Игры не найдены' : showExcluded ? 'Исключений пока нет' : 'Пока нет распознанных игр'}</h3><p>{search ? 'Попробуйте другое название.' : showExcluded ? 'Исключённые игры перестают учитываться и блокироваться.' : 'Запустите игру. Afterwork добавит её в список, если распознает.'}</p></div>}
      <HistoryNavigation page={visiblePage} more={(visiblePage + 1) * 50 < visibleGames.length} change={setCatalogPage} nextLabel="Ещё игры" />
    </section>
    <section className="panel list-panel game-session-list">
      <div className="card-heading"><span className="card-icon"><History size={19} /></span><div><h2>История игр</h2><p>Сессии за выбранный период</p></div></div>
      {report?.sessions.length ? <div className="history-list">{report.sessions.map(session => <div className="history-row" key={session.id}>
        <div className="game-history-label"><GameIcon path={session.game} status={data.gameIcons?.[session.game]} /><div><b>{gamesByPath.get(session.game) ?? sessionName(session.game)}</b><small>{dateTime.format(session.start * 1000)}</small></div></div><span>{duration(session.end - session.start)}</span>
      </div>)}</div> : <p className="empty-inline">{result?.error ?? (report ? 'За этот период нет игровых сессий.' : 'Загрузка истории…')}</p>}
      <HistoryNavigation page={historyPage} more={!!report?.more} change={setHistoryPage} />
    </section>
    <div className="info-line"><ShieldCheck size={17} />{data.settings.gameBlocking ? 'Статистика хранится на этом компьютере. Блокировка применяется к новым запускам.' : 'Статистика хранится на этом компьютере.'}</div>
  </div>
}
