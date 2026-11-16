import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import {
  Archive, BookOpen, Brain, BriefcaseBusiness, Check, ChevronDown, CodeXml, Dumbbell, Heart,
  Languages, Layers, Music, Palette, Pencil, Plus, RotateCcw, Search, Sparkles, Target,
} from 'lucide-react'
import { duration, type Activity, type ActivityDetails, type ActivityIcon, type ActivityColor, type ActivitySummaries } from './types'

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
  const listId = useId()
  const visible = expanded && !disabled
  const matching = activities.filter(item => !item.archived && matches(item, query))
  const recent = query.trim() ? [] : matching.filter(item => summaries[item.id])
    .sort((a, b) => summaries[b.id].lastActive - summaries[a.id].lastActive || byName.compare(a.name, b.name)).slice(0, 4)
  const others = matching.filter(item => !recent.some(entry => entry.id === item.id)).sort((a, b) => byName.compare(a.name, b.name))
  const options = [...recent, ...others]
  const pointed = options.find(item => item.id === highlight)
  const optionId = (id: number) => listId + '-' + id

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
      <ActivityMark activity={selected} /><span>{selected?.name ?? 'Выберите занятие'}</span><ChevronDown size={15} className="picker-chevron" />
    </button>
    {visible && <div className="picker-popover">
      <label className="activity-search picker-search"><Search size={16} /><input autoFocus role="combobox" aria-label="Поиск занятий" aria-autocomplete="list" aria-expanded="true" aria-controls={listId} aria-activedescendant={pointed ? optionId(pointed.id) : undefined}
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

export function ActivityEditor({ activity, initialName, error, busy, close, save }: {
  activity?: Activity; initialName: string; error: string; busy: boolean
  close: () => void; save: (details: ActivityDetails) => void
}) {
  const [details, setDetails] = useState<ActivityDetails>({ name: activity?.name ?? initialName, icon: activity?.icon ?? 'sparkles', color: activity?.color ?? 'violet' })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!busy && details.name.trim()) save(details)
  }
  return <form className="activity-editor" onSubmit={submit}>
    <ActivityMark activity={details} big />
    <h2>{activity ? 'Редактировать занятие' : 'Новое занятие'}</h2>
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
    {error && <p className="error" role="alert">{error}</p>}
    <div className="dialog-actions"><button type="button" className="button secondary" disabled={busy} onClick={close}>Отмена</button><button className="button primary" disabled={busy || !details.name.trim()}>{activity ? 'Сохранить' : 'Создать'}</button></div>
  </form>
}

export function Activities({ activities, summaries, busy, create, edit, archive }: {
  activities: Activity[]; summaries: ActivitySummaries; busy: boolean
  create: () => void; edit: (activity: Activity) => void; archive: (activity: Activity) => void
}) {
  const [archived, setArchived] = useState(false)
  const [query, setQuery] = useState('')
  const archiveCount = activities.filter(item => item.archived).length
  const visible = activities.filter(item => item.archived === archived && matches(item, query)).sort((a, b) => byName.compare(a.name, b.name))
  return <div className="page activities-page">
    <div className="heading"><div><h1>Занятия</h1><p>Выберите, чему уделяете время, и настройте список занятий.</p></div><button className="button primary" onClick={create} disabled={busy}><Plus size={17} />Новое занятие</button></div>
    <section className="panel activity-directory" aria-label="Список занятий">
      <div className="activity-toolbar">
        <div className="activity-tabs" role="group" aria-label="Состояние занятий">
          <button type="button" aria-pressed={!archived} onClick={() => setArchived(false)}>Активные <small>{activities.length - archiveCount}</small></button>
          <button type="button" aria-pressed={archived} onClick={() => setArchived(true)}>Архив <small>{archiveCount}</small></button>
        </div>
        <label className="activity-search"><Search size={16} /><input aria-label="Поиск по названию" placeholder="Поиск по названию…" value={query} onChange={event => setQuery(event.target.value)} /></label>
      </div>
      {visible.length > 0 ? <>
        <div className="activity-grid activity-columns" aria-hidden="true"><span>ЗАНЯТИЕ</span><span>ВСЁ ВРЕМЯ</span><span>ПОСЛЕДНЯЯ АКТИВНОСТЬ</span><span /></div>
        <ul className="activity-rows">
          {visible.map(item => {
            const summary = summaries[item.id]
            const last = summary ? dateTime.format(new Date(summary.lastActive * 1000)) : 'Ещё не было'
            const ArchiveIcon = item.archived ? RotateCcw : Archive
            const archiveLabel = item.archived ? 'Восстановить' : 'В архив'
            return <li className="activity-grid activity-row" key={item.id}>
              <div className="activity-name"><ActivityMark activity={item} big /><strong>{item.name}</strong></div>
              <span className="activity-total" aria-label={'Всего ' + duration(summary?.seconds ?? 0)}>{duration(summary?.seconds ?? 0)}</span>
              <span className="activity-last" aria-label={'Последняя активность: ' + last}>{last}</span>
              <div className="activity-row-actions"><button className="icon-button" disabled={busy} title="Редактировать" aria-label={'Редактировать «' + item.name + '»'} onClick={() => edit(item)}><Pencil size={15} /></button>
                <button className="button secondary archive-button" disabled={busy} title={archiveLabel} aria-label={archiveLabel + ' «' + item.name + '»'} onClick={() => archive(item)}><ArchiveIcon size={15} /><span>{archiveLabel}</span></button></div>
            </li>
          })}
        </ul>
      </> : <div className="empty"><Layers size={26} /><h3>{query.trim() ? 'Ничего не найдено' : archived ? 'Архив пуст' : 'Нет активных занятий'}</h3><p>{query.trim() ? 'Попробуйте поискать по другому названию.' : archived ? 'Сюда попадут занятия, которые вы отправите в архив.' : 'Создайте занятие или восстановите его из архива.'}</p></div>}
      {archived && visible.length > 0 && <div className="activity-archive-note"><Archive size={15} />Время и история архивных занятий остаются в статистике.</div>}
    </section>
  </div>
}
