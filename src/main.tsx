import { useCallback, useEffect, useLayoutEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { desktop } from './desktop'
import { Dialog } from './dialog'
import {
  Activity as Pulse, ArrowUpRight, BarChart3, Bell, Check, ChevronDown, CircleAlert, Clock3,
  Flag, Gamepad2, ImagePlus, Layers, LayoutGrid, LockKeyhole, Minus, Pause, Play, Plus, Settings2, ShieldCheck,
  Square, Sun, Sunset, Timer, Trash2, Trophy, X, Zap,
} from 'lucide-react'
import '@fontsource-variable/inter'
import '@fontsource-variable/manrope'
import '@fontsource/jetbrains-mono/500.css'
import './style.css'
import './activities.css'
import './priorities.css'
import './games.css'
import './achievements.css'
import { Activities, ActivityEditor, ActivityPicker } from './activities'
import { Achievements, AchievementUnlock } from './achievements'
import { GameIcon, Games } from './games'
import { PriorityEditor, PriorityGoals } from './priorities'
import { Statistics } from './statistics'
import { UpdateSettings, useUpdates } from './updates'
import { canBlockGame, clock, dateOf, stopwatch, type Action, type Activity, type ActivityDetails, type Game, type GoalRename, type Settings, type Snapshot } from './types'

const pages = [
  { id: 'home', label: 'Главная', icon: LayoutGrid },
  { id: 'activities', label: 'Занятия', icon: Layers },
  { id: 'statistics', label: 'Статистика', icon: BarChart3 },
  { id: 'achievements', label: 'Достижения', icon: Trophy },
  { id: 'games', label: 'Игры', icon: Gamepad2 },
  { id: 'settings', label: 'Настройки', icon: Settings2 },
] as const
type Page = typeof pages[number]['id']
type Modal = { type: 'activity'; activity?: Activity; initialName: string; selectNew?: boolean; goalSetup?: { activityId?: number } }
  | { type: 'priorities'; activityId?: number; focusGoal?: string } | { type: 'finish' } | { type: 'remove' | 'icon'; game: Game }
const todayFormat = new Intl.DateTimeFormat('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' })

function reconcileSnapshot(previous: Snapshot | undefined, next: Snapshot) {
  if (!previous) return next
  let changed = false
  for (const key of Object.keys(next) as (keyof Snapshot)[]) {
    const before = previous[key]
    const after = next[key]
    if (Object.is(before, after)) continue
    if (after && typeof after === 'object' && JSON.stringify(before) === JSON.stringify(after)) {
      Object.assign(next, { [key]: before })
    } else changed = true
  }
  return changed ? next : previous
}

function App() {
  const [page, setPage] = useState<Page>('home')
  const content = useRef<HTMLElement>(null)
  useLayoutEffect(() => { content.current?.scrollTo(0, 0) }, [page])
  const [data, setData] = useState<Snapshot>()
  const [selection, setSelection] = useState<number>()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [modal, setModal] = useState<Modal>()
  const prioritiesTrigger = useRef<HTMLButtonElement>(null)
  useLayoutEffect(() => {
    if (!modal && !busy && prioritiesTrigger.current) {
      prioritiesTrigger.current.focus()
      prioritiesTrigger.current = null
    }
  }, [busy, modal])
  const [completedGoals, setCompletedGoals] = useState<string[]>([])
  const [text, setText] = useState('')
  const [newAwards, setNewAwards] = useState<string[]>([])
  const [updateConfirmation, setUpdateConfirmation] = useState<string>()
  const { status: updateStatus, refresh: refreshUpdates } = useUpdates()
  const knownAwards = useRef<Set<string> | undefined>(undefined)
  const sequence = useRef(0)
  const fetching = useRef(0)
  const pending = useRef(false)
  const refresh = useCallback(async () => {
    const request = ++sequence.current
    fetching.current = request
    try {
      const next = await desktop.snapshot({ report: page === 'statistics', games: page === 'games', achievements: page === 'achievements' })
      if (request === sequence.current) setData(previous => reconcileSnapshot(previous, next))
      return next
    } catch (cause) {
      if (request === sequence.current) setError(String(cause))
    } finally {
      if (fetching.current === request) fetching.current = 0
    }
  }, [page])

  useEffect(() => {
    void refresh()
    const update = () => { if (!document.hidden && !pending.current && !fetching.current) void refresh() }
    const interval = window.setInterval(update, page === 'home' ? 500 : 1500)
    document.addEventListener('visibilitychange', update)
    window.addEventListener('focus', update)
    return () => {
      ++sequence.current
      window.clearInterval(interval)
      document.removeEventListener('visibilitychange', update)
      window.removeEventListener('focus', update)
    }
  }, [page, refresh])

  useEffect(() => {
    if (!data) return
    const earned = Object.keys(data.awards)
    const previous = knownAwards.current
    knownAwards.current = new Set(earned)
    if (previous) {
      const added = earned.filter(id => !previous.has(id))
      if (added.length) setNewAwards(previous => [...new Set([...added, ...previous])])
    }
  }, [data?.awards])

  useEffect(() => {
    if (!newAwards.length) return
    const timeout = window.setTimeout(() => setNewAwards([]), 8500)
    return () => window.clearTimeout(timeout)
  }, [newAwards])

  const perform = async (job: () => Promise<void>) => {
    if (pending.current) return
    pending.current = true
    setBusy(true)
    setError('')
    try { await job() } catch (cause) { setError(String(cause)) }
    finally { pending.current = false; setBusy(false) }
  }

  const apply = async (action: Action) => {
    await desktop.apply(action)
    return refresh()
  }
  const showModal = (value: Modal) => { setCompletedGoals([]); setText(''); setError(''); setModal(value) }
  const openPriorities = (trigger: HTMLButtonElement, activityId?: number) => {
    prioritiesTrigger.current = trigger
    showModal({ type: 'priorities', activityId })
  }
  const closeModal = () => {
    if (busy) return
    setModal(modal?.type === 'activity' && modal.goalSetup ? { type: 'priorities', ...modal.goalSetup } : undefined)
    setError('')
  }
  const updateGameIcon = (game: Game, reset = false) => void perform(async () => {
    const file = reset ? null : await desktop.chooseIcon()
    if (!reset && typeof file !== 'string') return
    await desktop.setGameIcon(game.path, file)
    await refresh()
    setModal(undefined)
  })
  const saveActivity = (details: ActivityDetails, goalRenames: GoalRename[]) => {
    if (modal?.type !== 'activity') return
    void perform(async () => {
      const next = await apply(modal.activity ? { type: 'editActivity', id: modal.activity.id, details, goalRenames } : { type: 'addActivity', details })
      if (modal.selectNew) setSelection(next?.activities.at(-1)?.id)
      setModal(modal.goalSetup ? {
        type: 'priorities', activityId: modal.activity?.id ?? next?.activities.at(-1)?.id, focusGoal: details.goals?.at(-1),
      } : undefined)
    })
  }
  const submitModal = (event: FormEvent) => {
    event.preventDefault()
    void perform(async () => {
      if (!modal) return
      if (modal.type === 'finish') await apply({ type: 'finish', note: text, completedGoals })
      else if (modal.type === 'remove') await apply({ type: 'removeGame', path: modal.game.path })
      setModal(undefined)
    })
  }

  const win = desktop.window()
  const activeActivity = data?.activities.find(activity => activity.id === data.active?.activity)
  const finishGoals = activeActivity?.goalsEnabled ? activeActivity.goals ?? [] : []
  const runUpdate = (command: () => Promise<void>) => void perform(async () => { await command(); await refreshUpdates() })
  const confirmUpdate = () => {
    if (updateStatus?.phase === 'ready' && updateStatus.version) setUpdateConfirmation(updateStatus.version)
  }
  return <div className="app">
    <header className="titlebar" data-tauri-drag-region>
      <div className="wordmark" data-tauri-drag-region><span className="logo" data-tauri-drag-region><Pulse size={17} /></span>Afterwork<span className="word-dot" /></div>
      <div className="caption">
        <button aria-label="Свернуть" onClick={() => void win.minimize()}><Minus size={15} /></button>
        <button aria-label="Развернуть или восстановить" onClick={() => void win.toggleMaximize()}><Square size={11} /></button>
        <button className="caption-close" aria-label="Скрыть в трей" title="Скрыть в трей" onClick={() => void win.close()}><X size={16} /></button>
      </div>
    </header>
    <div className="workspace">
      <aside className="sidebar">
        <nav aria-label="Разделы">
          {pages.map(({ id, label, icon: Icon }) => <button key={id} className={page === id ? 'nav active' : 'nav'} title={label} aria-current={page === id ? 'page' : undefined} onClick={() => setPage(id)}><Icon size={18} /><span>{label}</span></button>)}
        </nav>
        <div className="sidebar-bottom"><span className="online" /><span>Работает в фоне</span></div>
      </aside>
      <main ref={content}>
        {page !== 'settings' && (updateStatus?.phase === 'ready' || updateStatus?.phase === 'installing') && <div className="update-banner" role="status">
          <span>{updateStatus.error ?? (updateStatus.phase === 'ready' ? `Afterwork ${updateStatus.version} готов к установке.` : 'Запускаем установщик Afterwork…')}</span>
          {updateStatus.phase === 'ready' && <button className="button" disabled={busy || !!modal} onClick={confirmUpdate}>Перезапустить и установить</button>}
        </div>}
        {!data ? <div className="loading">{error || 'Загрузка Afterwork…'}</div> : <>
          {page === 'home' && <Home data={data} busy={busy} selection={selection} select={setSelection} run={action => void perform(async () => { await apply(action) })} add={initialName => showModal({ type: 'activity', initialName, selectNew: true })} finish={() => showModal({ type: 'finish' })} games={() => setPage('games')} managePriorities={openPriorities} />}
          {page === 'activities' && <Activities activities={data.activities} summaries={data.activitySummaries} priorities={data.priorityGoals} busy={busy}
            prioritize={(activity, trigger) => openPriorities(trigger, activity.id)}
            create={() => showModal({ type: 'activity', initialName: '' })} edit={activity => showModal({ type: 'activity', activity, initialName: '' })}
            archive={activity => void perform(async () => {
              await apply({ type: 'archiveActivity', id: activity.id, archived: !activity.archived })
              if (!activity.archived && selection === activity.id) setSelection(undefined)
            })} />}
          {page === 'statistics' && <Statistics data={data} openAchievements={() => setPage('achievements')}
            shortenSession={(id, seconds) => apply({ type: 'shortenSession', id, seconds })} />}
          {page === 'achievements' && <Achievements data={data} />}
          {page === 'games' && <Games data={data} busy={busy} editIcon={game => showModal({ type: 'icon', game })} remove={game => showModal({ type: 'remove', game })} exclude={game => void perform(async () => {
            await apply({ type: 'excludeGame', path: game.path, excluded: !game.excluded })
          })} add={() => void perform(async () => {
            const path = await desktop.chooseGame()
            if (typeof path === 'string') await apply({ type: 'addGame', path })
          })} />}
          {page === 'settings' && <Preferences initial={data.settings} save={settings => void perform(async () => { await apply({ type: 'saveSettings', settings }) })} busy={busy} reportError={setError} updates={
            <UpdateSettings status={updateStatus} busy={busy} check={() => runUpdate(desktop.checkUpdate)} download={() => runUpdate(desktop.downloadUpdate)} confirm={confirmUpdate}
              toggle={<Toggle label="Автоматическое обновление" value={!!updateStatus?.autoUpdate} disabled={busy || !updateStatus || updateStatus.phase === 'disabled' || updateStatus.phase === 'installing'}
                change={() => runUpdate(() => desktop.setAutoUpdate(!updateStatus?.autoUpdate))} />} />
          } />}
        </>}
      </main>
    </div>
    {updateConfirmation && <Dialog label="Установка обновления" close={() => setUpdateConfirmation(undefined)}>
      <div className="dialog-heading"><span className="card-icon"><ArrowUpRight size={19} /></span><h2>Установить Afterwork {updateConfirmation}?</h2></div>
      <p>Afterwork закроется на время установки и откроется снова. Настройки, статистика и текущая сессия сохранятся. Запущенный таймер учтёт время перезапуска.</p>
      <div className="dialog-actions">
        <button autoFocus className="button" onClick={() => setUpdateConfirmation(undefined)}>Позже</button>
        <button className="button primary" disabled={busy || updateStatus?.phase !== 'ready'} onClick={() => {
          setUpdateConfirmation(undefined)
          runUpdate(() => desktop.installUpdate(updateConfirmation))
        }}>Перезапустить и установить</button>
      </div>
    </Dialog>}
    {newAwards.length > 0 && <AchievementUnlock key={newAwards.join()} ids={newAwards} close={() => setNewAwards([])} open={() => { setPage('achievements'); setNewAwards([]) }} />}
    {error && data && !modal && <div role="alert" className="toast"><CircleAlert size={17} /><span>{error}</span><button aria-label="Закрыть сообщение" onClick={() => setError('')}><X size={16} /></button></div>}
    {modal && <Dialog key={modal.type} close={closeModal} className={modal.type === 'priorities' ? 'priority-dialog' : undefined}
      label={modal.type === 'activity' ? (modal.goalSetup ? 'Настройка цели' : modal.activity ? 'Редактировать занятие' : 'Новое занятие') : modal.type === 'priorities' ? 'Ближайшие цели' : modal.type === 'finish' ? 'Завершение сессии' : modal.type === 'icon' ? 'Иконка игры' : 'Удалить игру?'}>
      {modal.type === 'priorities' && data ? <PriorityEditor initial={data.priorityGoals} activities={data.activities} activityId={modal.activityId} focusGoal={modal.focusGoal}
        busy={busy} error={error} close={closeModal}
        save={goals => void perform(async () => { await apply({ type: 'setPriorityGoals', goals }); setModal(undefined) })}
        addGoal={(activity, activityId) => showModal({ type: 'activity', activity, initialName: '', goalSetup: { activityId } })} />
      : modal.type === 'activity' ? <ActivityEditor activity={modal.activity} initialName={modal.initialName} goalSetup={!!modal.goalSetup}
        error={error} busy={busy} close={closeModal} save={saveActivity} /> : modal.type === 'icon' ? <div>
        <h2>Иконка игры</h2>
        <div className="game-icon-preview"><GameIcon path={modal.game.path} status={data?.gameIcons?.[modal.game.path]} large /><div><b>{modal.game.name}</b><small>{data?.gameIcons?.[modal.game.path]?.custom ? 'Своя иконка' : 'Автоматический выбор'}</small></div></div>
        <p>Поддерживаются PNG, JPG и ICO до 8 МБ.</p>
        {error && <p className="error" role="alert">{error}</p>}
        <div className="dialog-actions game-icon-actions">
          <button className="button secondary" disabled={busy} onClick={closeModal}>Закрыть</button>
          {data?.gameIcons?.[modal.game.path]?.custom && <button className="button secondary" disabled={busy} title="Вернуть автоматическую" aria-label="Вернуть автоматическую" onClick={() => updateGameIcon(modal.game, true)}>Сбросить</button>}
          <button className="button primary" disabled={busy} onClick={() => updateGameIcon(modal.game)}><ImagePlus size={16} />Выбрать файл</button>
        </div>
      </div> : <form className="session-dialog" onSubmit={submitModal}>
        <div className="dialog-heading">
          <span className="card-icon">{modal.type === 'finish' ? <Check size={18} /> : <Trash2 size={18} />}</span>
          <div><h2>{modal.type === 'finish' ? 'Завершение сессии' : 'Удалить игру?'}</h2>
            {modal.type === 'finish' && <p>{activeActivity?.name} · {stopwatch(data?.elapsed ?? 0)}</p>}</div>
        </div>
        <div className="dialog-body">
          {modal.type === 'finish' && <>
            {finishGoals.length > 0 && <fieldset className="finish-goals" aria-labelledby="finish-goals-title" aria-describedby="finish-goals-hint">
              <div className="finish-goals-heading">
                <span className="finish-goals-icon"><Flag size={16} aria-hidden="true" /></span>
                <div><strong id="finish-goals-title">Выполненные цели</strong><p id="finish-goals-hint">Отметьте цели, которых достигли за сессию.</p></div>
                <output aria-label="Выбрано целей" className="finish-goals-count">{completedGoals.length} / {finishGoals.length}</output>
              </div>
              <div className="finish-goal-options">
                {finishGoals.map((goal, index) => <label key={goal} className="finish-goal-option">
                  <input type="checkbox" autoFocus={index === 0} checked={completedGoals.includes(goal)} disabled={busy}
                    onChange={event => {
                      const checked = event.target.checked
                      setCompletedGoals(previous => checked ? [...previous, goal] : previous.filter(item => item !== goal))
                    }} />
                  <span>{goal}</span><span className="finish-goal-check" aria-hidden="true"><Check size={14} /></span>
                </label>)}
              </div>
            </fieldset>}
            <label className="field">
              <span className="finish-note-title">Что удалось сделать? <span className="muted">По желанию</span></span>
              <textarea autoFocus={!finishGoals.length} rows={3} maxLength={2000} value={text} placeholder="Запишите результат сессии…" onChange={event => setText(event.target.value)} />
            </label>
          </>}
          {modal.type === 'remove' && <p>Игра «{modal.game.name}» будет удалена из списка блокировки.</p>}
          {error && <p className="error" role="alert">{error}</p>}
        </div>
        <div className="dialog-actions"><button className="button secondary" type="button" disabled={busy} onClick={closeModal}>Отмена</button><button className={'button ' + (modal.type === 'remove' ? 'danger' : 'primary')} disabled={busy}>{modal.type === 'remove' ? 'Удалить' : 'Сохранить сессию'}</button></div>
      </form>}
    </Dialog>}
  </div>
}

export function Heading({ title, subtitle, action }: { title: string; subtitle: string; action?: ReactNode }) {
  return <div className="heading"><div><h1>{title}</h1><p>{subtitle}</p></div>{action}</div>
}

function Home({ data, busy, selection, select, run, add, finish, games, managePriorities }: {
  data: Snapshot; busy: boolean; selection?: number; select: (id: number) => void
  run: (action: Action) => void; add: (name: string) => void; finish: () => void; games: () => void
  managePriorities: (trigger: HTMLButtonElement) => void
}) {
  const startButton = useRef<HTMLButtonElement>(null)
  const running = data.active?.running != null
  const selected = data.activities.find(item => data.active ? item.id === data.active.activity : item.id === selection && !item.archived)
  const goals = selected?.goalsEnabled ? selected.goals ?? [] : []
  const percent = Math.min(100, Math.floor(data.seconds / (data.target * 2) * 100))
  const unlockedDay = data.seconds >= 2 * data.target
  const left = Math.max(0, data.target * data.phase - data.seconds)
  const { settings } = data
  const hasFailures = Object.keys(data.failures).length > 0
  const tracked = data.games.some(game => !game.excluded)
  const blockable = data.games.some(game => !game.excluded && canBlockGame(game))
  const status = !tracked ? 'Нет игр под контролем' : !blockable ? 'Учёт игрового времени' : data.locked ? (hasFailures ? 'Блокировка с ограничениями' : 'Игры заблокированы') : 'Игры разблокированы'
  return <div className="page home">
    <Heading title="Время для важного" subtitle="Запускайте фокус-сессии и следите за дневной целью." action={<span className="date-chip"><Clock3 size={15} /><span>{todayFormat.format(dateOf(data.date))}</span></span>} />
    <PriorityGoals goals={data.priorityGoals} activities={data.activities} busy={busy} sessionActive={!!data.active} manage={managePriorities}
      prepare={activity => { select(activity); requestAnimationFrame(() => startButton.current?.focus()) }} />
    <div className="hero-grid">
      <section className="panel timer-panel">
        <div className="card-heading timer-heading"><span className="card-icon"><Timer size={19} /></span><h2>Фокус-сессия</h2>
          <div className="timer-state"><span className={running ? 'state-dot running' : 'state-dot'} />{running ? 'Идёт' : data.active ? 'На паузе' : 'Готова к старту'}</div>
        </div>
        <div className="timer-body">
          <div className="digits" aria-label={'Время: ' + stopwatch(data.elapsed)}>{stopwatch(data.elapsed).split(':').map((value, index) => <span key={index}>{index > 0 && <i>:</i>}{value}</span>)}</div>
          <div className="activity-line">
            <ActivityPicker activities={data.activities} summaries={data.activitySummaries} selected={selected} disabled={!!data.active || busy} choose={select} create={add} />
            {!data.active && <button className="icon-button activity-quick-create" title="Новое занятие" aria-label="Новое занятие" disabled={busy} onClick={() => add('')}><Plus size={17} /></button>}
          </div>
          <div className="timer-actions">
            {data.active ? <>
              <button className="button primary" disabled={busy} onClick={() => run({ type: running ? 'pause' : 'resume' })}>{running ? <Pause size={17} /> : <Play size={17} />}{running ? 'Пауза' : 'Продолжить'}</button>
              <button className="button secondary" disabled={busy} onClick={finish}><Square size={14} />Завершить</button>
            </> : <button ref={startButton} className="button primary start" disabled={busy || !selected} onClick={() => selected && run({ type: 'start', activity: selected.id })}><Play size={17} fill="currentColor" />Начать сессию</button>}
          </div>
        </div>
      </section>
      <section className="panel progress-panel">
        <div className="card-heading"><span className="card-icon"><Zap size={19} /></span><h2>Прогресс дня</h2></div>
        <div className="progress-summary"><div><strong>{Math.floor(data.seconds / 60)}</strong><span> / {settings.minutes * 2} мин</span></div><span className="percentage">{percent}%</span></div>
        <div className="progress-track" role="progressbar" aria-label="Прогресс дня" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}><span style={{ width: percent + '%' }} /><i /></div>
        <div className="stages">
          {[1, 2].map(stage => {
            const done = data.seconds >= data.target * stage
            const current = data.phase === stage
            const Icon = done ? Check : stage === 1 ? Sun : Sunset
            return <div className={'stage ' + (done ? 'done' : current ? 'current' : '')} key={stage}>
              <span className="stage-icon"><Icon size={17} /></span><div><b>{stage === 1 ? 'Первый этап' : 'Второй этап'}</b><small>{clock(stage === 1 ? settings.first : settings.second)} · {settings.minutes} мин</small></div>
              <span className="stage-state">{done ? 'Готово' : current ? 'Активен' : stage > data.phase ? 'Позже' : 'Не выполнен'}</span>
            </div>
          })}
        </div>
      </section>
    </div>
    {goals.length > 0 && <details className="panel session-goals" key={selected?.id}>
      <summary><Flag size={17} /><strong>Цели занятия</strong><span>{goals.length}</span><ChevronDown size={16} className="session-goals-chevron" /></summary>
      <ul aria-label="Цели занятия" tabIndex={0}>{goals.map(goal => <li key={goal}>{goal}</li>)}</ul>
    </details>}
    <section className={'panel unlock ' + (!data.locked && blockable ? 'unlocked' : '')}>
      <span className="unlock-icon">{blockable && data.locked ? <LockKeyhole size={22} /> : blockable ? <ShieldCheck size={22} /> : <Gamepad2 size={22} />}</span>
      <div><h2>{status}</h2><p>{!tracked ? 'Afterwork добавляет распознанные игры в список автоматически.' : !blockable ? 'Игровые сессии сохраняются в разделе «Игры».' : hasFailures ? 'Проверьте состояние отдельных файлов в разделе «Игры».' : data.locked ? 'До разблокировки осталось ' + Math.ceil(left / 60) + ' мин работы.' : unlockedDay ? 'Дневная цель выполнена. Игры доступны до ' + clock(settings.first) + '.' : 'Игры доступны до ' + clock(settings.second) + '. Второй этап можно выполнить заранее.'}</p></div>
      <button className="button secondary unlock-button" onClick={games}>Мои игры <ArrowUpRight size={16} /></button>
    </section>
  </div>
}

function Preferences({ initial, save, busy, reportError, updates }: { initial: Settings; save: (settings: Settings) => void; busy: boolean; reportError: (error: string) => void; updates: ReactNode }) {
  const [first, setFirst] = useState(clock(initial.first))
  const [second, setSecond] = useState(clock(initial.second))
  const [minutes, setMinutes] = useState(String(initial.minutes))
  const [notifications, setNotifications] = useState(initial.notifications)
  const [autostart, setAutostart] = useState<boolean | null>(null)
  const [changing, setChanging] = useState(false)
  useEffect(() => { desktop.autostart().then(setAutostart).catch(error => reportError(String(error))) }, [reportError])
  const toggle = async () => {
    setChanging(true)
    try { setAutostart(await desktop.autostart(!autostart)) }
    catch (error) { reportError(String(error)) }
    finally { setChanging(false) }
  }
  const parse = (time: string) => { const [hours, mins] = time.split(':').map(Number); return hours * 60 + mins }
  const dirty = first !== clock(initial.first) || second !== clock(initial.second) || Number(minutes) !== initial.minutes || notifications !== initial.notifications
  return <div className="page">
    <Heading title="Настройки" subtitle="Настройте расписание и поведение Afterwork." />
    <form className="preferences" onSubmit={event => { event.preventDefault(); save({ first: parse(first), second: parse(second), minutes: Number(minutes), notifications }) }}>
      <section className="panel list-panel">
        <div className="card-heading"><span className="card-icon"><Clock3 size={19} /></span><div><h2>Расписание</h2><p>С началом первого этапа начинается новый день.</p></div></div>
        <div className="settings-grid">
          <label className="field">Первый этап<input type="time" required value={first} onChange={event => setFirst(event.target.value)} /></label>
          <label className="field">Второй этап<input type="time" required value={second} onChange={event => setSecond(event.target.value)} /></label>
          <label className="field">Минут на этап<input required type="number" min={1} max={240} value={minutes} onChange={event => setMinutes(event.target.value)} /></label>
        </div>
      </section>
      <section className="panel list-panel">
        <div className="card-heading"><span className="card-icon"><Bell size={19} /></span><div><h2>В фоне</h2><p>Уведомления и вход в Windows</p></div></div>
        <div className="setting-row"><div><b>Ежечасное напоминание</b><p>«Пора улучшить жизнь :3» до выполнения второго этапа.</p></div><Toggle label="Ежечасное напоминание" value={notifications} change={() => setNotifications(!notifications)} /></div>
        <div className="setting-row"><div><b>Автозапуск с Windows</b><p>Открывать Afterwork в трее при входе в систему.</p></div><Toggle label="Автозапуск с Windows" value={!!autostart} disabled={changing || autostart === null} change={() => void toggle()} /></div>
      </section>
      <div className="form-footer"><p role="status">{dirty ? 'Есть несохранённые изменения' : 'Настройки сохранены'}</p><button className={`button ${dirty ? 'primary' : 'secondary'}`} disabled={busy || !dirty}><Check size={17} />Сохранить настройки</button></div>
    </form>
    {updates}
  </div>
}

function Toggle({ label, value, change, disabled }: { label: string; value: boolean; change: () => void; disabled?: boolean }) {
  return <button type="button" className="toggle" role="switch" aria-label={label} aria-checked={value} disabled={disabled} onClick={change}><span /></button>
}

const render = () => createRoot(document.getElementById('root')!).render(<App />)
if (import.meta.env.DEV && import.meta.env.MODE === 'preview') {
  void import('./preview').then(({ installPreview }) => {
    installPreview()
    render()
  })
} else {
  render()
}
