import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import { HistoryNavigation } from './history'
import {
  Archive, BookOpen, Brain, BriefcaseBusiness, Check, ChevronDown, CodeXml, Dumbbell, Heart,
  Flag, Languages, Layers, Music, Palette, Pencil, Plus, RotateCcw, Search, Sparkles, Target, Trash2, X,
} from 'lucide-react'
import { duration, type Activity, type ActivityDetails, type ActivityIcon, type ActivityColor, type ActivitySummaries, type GoalRename } from './types'

const icons = {
  sparkles: [Sparkles, 'Искры'], briefcase: [BriefcaseBusiness, 'Портфель'], book: [BookOpen, 'Книга'],
  code: [CodeXml, 'Код'], pencil: [Pencil, 'Карандаш'], palette: [Palette, 'Палитра'], music: [Music, 'Музыка'],
  dumbbell: [Dumbbell, 'Тренировка'], languages: [Languages, 'Языки'], brain: [Brain, 'Мышление'],
  target: [Target, 'Цель'], heart: [Heart, 'Сердце'],
} satisfies Record<ActivityIcon, readonly [typeof Sparkles, string]>

const colors: Record<ActivityColor, string> = {
  violet: 'Фиолетовый', blue: 'Синий', cyan: 'Бирюзовый', green: 'Зелёный',
  amber: 'Янтарный', orange: 'Оранжевый', rose: 'Розовый', slate: 'Серый',
}
const byName = new Intl.Collator('ru', { numeric: true })
const dateTime = new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium', timeStyle: 'short' })
const matches = (activity: Activity, query: string) => activity.name.toLocaleLowerCase('ru').includes(query.trim().toLocaleLowerCase('ru'))
const maxGoals = 100
const maxGoalLength = 120

export function ActivityMark({ activity, big = false }: { activity?: ActivityDetails; big?: boolean }) {
  const [Icon] = icons[activity?.icon ?? 'sparkles']
  return <span className={'activity-mark' + (big ? ' big' : '')} data-color={activity?.color ?? 'violet'} aria-hidden="true"><Icon size={big ? 19 : 16} /></span>
}

export function ActivityPicker({ activities, summaries, selected, disabled, choose, create }: {
  activities: Activity[]; summaries: ActivitySummaries; selected?: Activity; disabled: boolean
  choose: (id: number) => void; create: (name: string) => void
}) {
  const [expanded, setExpanded] = useState(false)
  const [query, setQuery] = useState('')
  const [highlight, setHighlight] = useState<number>()
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const popover = useRef<HTMLDivElement>(null)
  const search = useRef<HTMLInputElement>(null)
  const listId = useId()
  const visible = expanded && !disabled
  const { matching, recent, others } = useMemo(() => {
    if (!visible) return { matching: [], recent: [], others: [] }
    const matching = activities.filter(item => !item.archived && matches(item, query))
    const recent = query.trim() ? [] : matching.filter(item => summaries[item.id])
      .sort((a, b) => summaries[b.id].lastActive - summaries[a.id].lastActive || byName.compare(a.name, b.name)).slice(0, 4)
    const others = matching.filter(item => !recent.some(entry => entry.id === item.id)).sort((a, b) => byName.compare(a.name, b.name))
    return { matching, recent, others }
  }, [activities, query, summaries, visible])
  const options = [...recent, ...others]
  const pointed = options.find(item => item.id === highlight)
  const optionId = (id: number) => listId + '-' + id

  useLayoutEffect(() => {
    if (!visible) return
    const button = trigger.current
    const popup = popover.current
    const viewport = button?.closest('main')
    if (!button || !popup || !viewport) return
    const position = () => {
      const anchor = button.getBoundingClientRect()
      const bounds = viewport.getBoundingClientRect()
      const below = bounds.bottom - anchor.bottom - 16
      const above = anchor.top - bounds.top - 16
      const opensUp = below < 304 && above > below
      popup.dataset.placement = opensUp ? 'above' : 'below'
      popup.style.maxHeight = Math.max(0, opensUp ? above : below) + 'px'
    }
    position()
    search.current?.focus({ preventScroll: true })
    viewport.addEventListener('scroll', position)
    window.addEventListener('resize', position)
    return () => {
      viewport.removeEventListener('scroll', position)
      window.removeEventListener('resize', position)
    }
  }, [visible])

  useEffect(() => {
    if (!visible) return
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setExpanded(false)
    }
    document.addEventListener('pointerdown', outside)
    return () => document.removeEventListener('pointerdown', outside)
  }, [visible])

  function openPicker() {
    setQuery('')
    setHighlight(selected?.archived ? undefined : selected?.id)
    setExpanded(true)
  }

  function closePicker() {
    setExpanded(false)
    trigger.current?.focus()
  }

  function select(item: Activity) {
    choose(item.id)
    closePicker()
  }

  function keySearch(event: KeyboardEvent<HTMLInputElement>) {
    if (event.nativeEvent.isComposing) return
    if (event.key === 'Enter' && (pointed || options.length)) {
      event.preventDefault()
      select(pointed ?? options[0])
    }
    if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && options.length) {
      event.preventDefault()
      const position = options.findIndex(item => item.id === highlight)
      const next = event.key === 'ArrowDown' ? (position + 1) % options.length : (position < 0 ? options.length - 1 : (position - 1 + options.length) % options.length)
      const id = options[next].id
      setHighlight(id)
      document.getElementById(optionId(id))?.scrollIntoView({ block: 'nearest' })
    }
  }

  const option = (item: Activity) => <div key={item.id} id={optionId(item.id)} role="option" aria-selected={selected?.id === item.id} className={'activity-option' + (pointed?.id === item.id ? ' pointed' : '')}
    onPointerMove={() => setHighlight(item.id)} onMouseDown={event => event.preventDefault()} onClick={() => select(item)}>
    <ActivityMark activity={item} /><span>{item.name}</span>{selected?.id === item.id && <Check size={15} className="option-check" />}
  </div>

  return <div className="activity-picker" ref={root}
    onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) setExpanded(false) }}
    onKeyDown={event => { if (visible && event.key === 'Escape') { event.stopPropagation(); closePicker() } }}>
    <button type="button" ref={trigger} className="picker-trigger" aria-label={selected ? 'Занятие: ' + selected.name : 'Выберите занятие'} aria-expanded={visible} aria-haspopup="listbox" aria-controls={visible ? listId : undefined}
      disabled={disabled} onClick={() => visible ? closePicker() : openPicker()}
      onKeyDown={event => { if (event.key === 'ArrowDown' && !visible) { event.preventDefault(); openPicker() } }}>
      <ActivityMark activity={selected} /><span title={selected?.name}>{selected?.name ?? 'Выберите занятие'}</span><ChevronDown size={15} className="picker-chevron" />
    </button>
    {visible && <div className="picker-popover" ref={popover}>
      <label className="activity-search picker-search"><Search size={16} /><input ref={search} role="combobox" aria-label="Поиск занятий" aria-autocomplete="list" aria-expanded="true" aria-controls={listId} aria-activedescendant={pointed ? optionId(pointed.id) : undefined}
        placeholder="Найти занятие…" maxLength={60} value={query} onChange={event => { setQuery(event.target.value); setHighlight(undefined) }} onKeyDown={keySearch} /></label>
      <div className="picker-list" role="listbox" aria-label="Доступные занятия" id={listId}>
        {recent.length > 0 && <div role="group" aria-label="Недавние"><div className="picker-caption" aria-hidden="true">Недавние</div>{recent.map(option)}</div>}
        {others.length > 0 && <div role="group" aria-label={recent.length ? 'Другие занятия' : 'Занятия'}>{recent.length > 0 && <div className="picker-caption" aria-hidden="true">Другие занятия</div>}{others.map(option)}</div>}
      </div>
      {!options.length && <p className="picker-empty">{query.trim() ? 'Ничего не найдено' : 'Нет доступных занятий'}</p>}
      <button type="button" className="picker-create" onClick={() => { closePicker(); create(query.trim()) }}><Plus size={17} />Создать занятие</button>
    </div>}
  </div>
}

export function ActivityEditor({ activity, initialName, goalSetup, error, busy, close, save }: {
  activity?: Activity; initialName: string; goalSetup?: boolean; error: string; busy: boolean
  close: () => void; save: (details: ActivityDetails, goalRenames: GoalRename[]) => void
}) {
  const [details, setDetails] = useState<ActivityDetails>({ goalsEnabled: goalSetup || (activity?.goalsEnabled ?? false), name: activity?.name ?? initialName, icon: activity?.icon ?? 'sparkles', color: activity?.color ?? 'violet' })
  const [goals, setGoals] = useState<{ name: string; original?: string }[]>(() => (activity?.goals ?? []).map(name => ({ name, original: name })))
  const [draft, setDraft] = useState<{ index: number | null; name: string } | undefined>(
    goalSetup && goals.length < maxGoals ? { index: null, name: '' } : undefined,
  )
  const addGoal = useRef<HTMLButtonElement>(null)
  const editButtons = useRef(new Map<number, HTMLButtonElement>())
  const draftErrorId = useId()
  const draftName = draft?.name.trim() ?? ''
  const duplicate = goals.some((goal, index) => goal.name === draftName && index !== draft?.index)
  const validDraft = !!draftName && !duplicate

  function updatedGoals() {
    if (!draft) return goals
    if (draft.index === null) return [...goals, { name: draftName }]
    return goals.map((goal, index) => index === draft.index ? { ...goal, name: draftName } : goal)
  }

  function closeDraft() {
    const index = draft?.index
    setDraft(undefined)
    requestAnimationFrame(() => {
      const trigger = index == null ? addGoal.current : editButtons.current.get(index)
      if (trigger && !trigger.disabled) trigger.focus()
      else editButtons.current.get(goals.length - 1)?.focus()
    })
  }

  function commitDraft() {
    if (!validDraft || busy) return
    setGoals(updatedGoals())
    closeDraft()
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!busy && details.name.trim() && (!draft || validDraft)) {
      const updated = updatedGoals()
      save({ ...details, goals: updated.map(goal => goal.name) }, updated.flatMap(({ original, name }) =>
        original !== undefined && original !== name ? [{ from: original, to: name }] : []))
    }
  }

  if (goalSetup) return <form className="activity-editor" onSubmit={submit}>
    <div className="dialog-heading"><ActivityMark activity={details} big /><div>
      <h2>{draft ? 'Добавить цель' : 'Включить цели'}</h2>
      {activity && <p>{activity.name}</p>}
    </div></div>
    <div className="dialog-body">
      {!activity && <label className="field">Название занятия<input autoFocus required maxLength={60}
        value={details.name} placeholder="Например, английский" onChange={event => setDetails({ ...details, name: event.target.value })} /></label>}
      {draft ? <label className="field">Название цели<input autoFocus={!!activity} required maxLength={maxGoalLength}
        value={draft.name} placeholder="Например, прочитать первую главу" aria-invalid={duplicate || undefined}
        aria-describedby={duplicate ? draftErrorId : undefined} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
        : <p>В этом занятии уже есть {maxGoals} целей. Включите их, чтобы выбрать ближайшие.</p>}
      {duplicate && <p className="error" id={draftErrorId} role="alert">Такая цель уже есть в занятии.</p>}
      {error && <p className="error" role="alert">{error}</p>}
    </div>
    <div className="dialog-actions">
      <button type="button" className="button secondary" disabled={busy} onClick={close}>Назад</button>
      <button className="button primary" disabled={busy || !details.name.trim() || (!!draft && !validDraft)}>{draft ? 'Сохранить цель' : 'Включить цели'}</button>
    </div>
  </form>

  const nameEditor = draft && <div className="goal-name-editor">
    <div className="goal-name-controls">
      <input autoFocus aria-label={draft.index === null ? 'Новая цель' : 'Название цели'}
        aria-invalid={duplicate || undefined} aria-describedby={duplicate ? draftErrorId : undefined}
        maxLength={maxGoalLength} placeholder="Название цели" value={draft.name} disabled={busy}
        onFocus={event => event.currentTarget.select()}
        onChange={event => setDraft({ ...draft, name: event.target.value })}
        onKeyDown={event => {
          if (event.nativeEvent.isComposing) return
          if (event.key === 'Enter') { event.preventDefault(); commitDraft() }
          if (event.key === 'Escape') { event.preventDefault(); closeDraft() }
        }} />
      <button type="button" className="goal-icon goal-apply" title="Подтвердить цель" aria-label="Подтвердить цель" disabled={busy || !validDraft} onClick={commitDraft}><Check size={16} /></button>
      <button type="button" className="goal-icon" title="Отменить правку цели" aria-label="Отменить правку цели" disabled={busy} onClick={closeDraft}><X size={16} /></button>
    </div>
    {duplicate && <p className="goal-name-error" id={draftErrorId} role="alert">Такая цель уже есть в занятии.</p>}
  </div>

  return <form className="activity-editor" onSubmit={submit}>
    <div className="dialog-heading"><ActivityMark activity={details} big /><h2>{activity ? 'Редактировать занятие' : 'Новое занятие'}</h2></div>
    <div className="dialog-body">
      <label className="field">Название<input autoFocus required maxLength={60} value={details.name} placeholder="Например, английский" onChange={event => setDetails({ ...details, name: event.target.value })} /></label>
      <fieldset><legend>Иконка</legend><div className="activity-icon-grid">
        {(Object.keys(icons) as ActivityIcon[]).map(icon => {
          const [Icon, label] = icons[icon]
          return <label className="appearance-icon" key={icon} title={label}><input type="radio" name="activity-icon" aria-label={label} checked={details.icon === icon} onChange={() => setDetails({ ...details, icon })} /><span><Icon size={18} /></span></label>
        })}
      </div></fieldset>
      <fieldset><legend>Цвет</legend><div className="activity-color-grid">
        {(Object.keys(colors) as ActivityColor[]).map(color => <label className="appearance-color" data-color={color} key={color} title={colors[color]}><input type="radio" name="activity-color" aria-label={colors[color]} checked={details.color === color} onChange={() => setDetails({ ...details, color })} /><span><Check size={13} /></span></label>)}
      </div></fieldset>
      <div className="goal-management">
        <div className="goal-management-heading">
          <Flag size={16} aria-hidden="true" />
          <strong id="manage-activity-goals">Использовать цели</strong>
          <button type="button" className="toggle" role="switch" aria-labelledby="manage-activity-goals"
            aria-checked={!!details.goalsEnabled} disabled={busy || !!draft}
            onClick={() => setDetails({ ...details, goalsEnabled: !details.goalsEnabled })}><span /></button>
        </div>
        {details.goalsEnabled && <div className="goal-management-body">
          {goals.length > 0 && <ul className="goal-editor-list" aria-label="Активные цели">
            {goals.map(({ name: goal }, index) => <li key={goal} className="goal-editor-row">
              {draft?.index === index ? nameEditor : <>
                <button type="button" className="goal-rename" title="Переименовать цель" aria-label={`Переименовать цель «${goal}»`}
                  ref={element => { if (element) editButtons.current.set(index, element); else editButtons.current.delete(index) }}
                  disabled={busy || !!draft} onClick={() => setDraft({ index, name: goal })}>
                  <span>{goal}</span><Pencil size={13} aria-hidden="true" />
                </button>
                <button type="button" className="goal-icon goal-remove" title="Удалить цель" aria-label={`Удалить цель «${goal}»`}
                  disabled={busy || !!draft} onClick={() => {
                    setGoals(goals.filter((_, position) => position !== index))
                    requestAnimationFrame(() => (editButtons.current.get(index) ?? addGoal.current)?.focus())
                  }}><Trash2 size={15} /></button>
              </>}
            </li>)}
          </ul>}
          {draft?.index === null ? nameEditor : goals.length < maxGoals && <button
            ref={addGoal} type="button" className="goal-add" disabled={busy || !!draft}
            onClick={() => setDraft({ index: null, name: '' })}><Plus size={16} />Добавить цель</button>}
        </div>}
      </div>
      {error && <p className="error" role="alert">{error}</p>}
    </div>
    <div className="dialog-actions"><button type="button" className="button secondary" disabled={busy} onClick={close}>Отмена</button><button className="button primary" disabled={busy || !details.name.trim() || (!!draft && !validDraft)}>{activity ? 'Сохранить' : 'Создать'}</button></div>
  </form>
}

function ActivityGoalSummary({ goals }: { goals: string[] }) {
  if (!goals.length) return null
  return <details className="activity-goal-preview">
    <summary><Flag size={12} />Цели <span>{goals.length}</span><ChevronDown size={12} className="goal-preview-chevron" /></summary>
    <ul aria-label="Активные цели" tabIndex={0}>{goals.map(goal => <li key={goal}>{goal}</li>)}</ul>
  </details>
}

export function Activities({ activities, summaries, busy, create, edit, archive }: {
  activities: Activity[]; summaries: ActivitySummaries; busy: boolean
  create: () => void; edit: (activity: Activity) => void; archive: (activity: Activity) => void
}) {
  const [archived, setArchived] = useState(false)
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(0)
  const search = useRef<HTMLInputElement>(null)
  const archiveCount = useMemo(() => activities.filter(item => item.archived).length, [activities])
  const visible = useMemo(() => activities.filter(item => item.archived === archived && matches(item, query))
    .sort((a, b) => byName.compare(a.name, b.name)), [activities, archived, query])
  const currentPage = Math.min(page, Math.max(0, Math.ceil(visible.length / 50) - 1))
  return <div className="page activities-page">
    <div className="heading"><div><h1>Занятия</h1><p>Выберите, чему уделяете время, и настройте список занятий.</p></div><button className="button primary" onClick={create} disabled={busy}><Plus size={17} />Новое занятие</button></div>
    <section className="panel activity-directory" aria-label="Список занятий">
      <div className="activity-directory-header">
        <div className="activity-toolbar">
          <div className="activity-tabs" role="group" aria-label="Состояние занятий">
            <button type="button" aria-pressed={!archived} onClick={() => { setArchived(false); setPage(0) }}>Активные <small>{activities.length - archiveCount}</small></button>
            <button type="button" aria-pressed={archived} onClick={() => { setArchived(true); setPage(0) }}>Архив <small>{archiveCount}</small></button>
          </div>
          <label className="activity-search"><Search size={16} /><input ref={search} aria-label="Поиск по названию" placeholder="Поиск по названию…" value={query} onChange={event => { setQuery(event.target.value); setPage(0) }} />{query && <button type="button" title="Очистить поиск" aria-label="Очистить поиск" onClick={() => { setQuery(''); setPage(0); search.current?.focus() }}><X size={15} /></button>}</label>
        </div>
        {visible.length > 0 && <div className="activity-grid activity-columns" aria-hidden="true"><span>ЗАНЯТИЕ</span><span>ВСЁ ВРЕМЯ</span><span>ПОСЛЕДНЯЯ АКТИВНОСТЬ</span><span /></div>}
      </div>
      {visible.length > 0 ?
        <ul className="activity-rows">
          {visible.slice(currentPage * 50, (currentPage + 1) * 50).map(item => {
            const summary = summaries[item.id]
            const last = summary ? dateTime.format(new Date(summary.lastActive * 1000)) : 'Ещё не было'
            const ArchiveIcon = item.archived ? RotateCcw : Archive
            const archiveLabel = item.archived ? 'Восстановить' : 'В архив'
            return <li className="activity-grid activity-row" key={item.id}>
              <div className="activity-name" data-color={item.color}>
                <ActivityMark activity={item} big />
                <div><strong title={item.name}>{item.name}</strong>{item.goalsEnabled && <ActivityGoalSummary goals={item.goals ?? []} />}</div>
              </div>
              <span className="activity-total" aria-label={'Всего ' + duration(summary?.seconds ?? 0)}>{duration(summary?.seconds ?? 0)}</span>
              <span className="activity-last" aria-label={'Последняя активность: ' + last}>{last}</span>
              <div className="activity-row-actions">
                <button className="icon-button" disabled={busy} title="Редактировать" aria-label={'Редактировать «' + item.name + '»'} onClick={() => edit(item)}><Pencil size={15} /></button>
                <button className="button secondary archive-button" disabled={busy} title={archiveLabel} aria-label={archiveLabel + ' «' + item.name + '»'} onClick={() => archive(item)}><ArchiveIcon size={15} /><span>{archiveLabel}</span></button></div>
            </li>
          })}
        </ul>
      : <div className="empty"><Layers size={26} /><h3>{query.trim() ? 'Ничего не найдено' : archived ? 'Архив пуст' : 'Нет активных занятий'}</h3><p>{query.trim() ? 'Попробуйте поискать по другому названию.' : archived ? 'Сюда попадут занятия, которые вы отправите в архив.' : 'Создайте занятие или восстановите его из архива.'}</p></div>}
      <HistoryNavigation page={currentPage} more={(currentPage + 1) * 50 < visible.length} change={setPage} nextLabel="Ещё занятия" />
      {archived && visible.length > 0 && <div className="activity-archive-note"><Archive size={15} />Время и история архивных занятий остаются в статистике.</div>}
    </section>
  </div>
}
