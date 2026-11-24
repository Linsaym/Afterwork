import { useEffect, useRef, useState } from 'react'
import { ArrowDown, ArrowRight, ArrowUp, ArrowLeftRight, Check, Pin, Plus, Search, X } from 'lucide-react'
import { ActivityMark } from './activities'
import { priorityGoalLimit, samePriorityGoal, type Activity, type PriorityGoal } from './types'

const keyOf = ({ activity, goal }: PriorityGoal) => JSON.stringify([activity, goal])
const hasGoals = (activity: Activity) => !activity.archived && activity.goalsEnabled && !!activity.goals?.length

export function PriorityGoals({ goals, activities, busy, sessionActive, manage, prepare }: {
  goals: PriorityGoal[]; activities: Activity[]; busy: boolean; sessionActive: boolean
  manage: (trigger: HTMLButtonElement) => void; prepare: (activity: number) => void
}) {
  const canChoose = activities.some(hasGoals)
  return <section className="panel priority-panel" aria-labelledby="priority-heading">
    <div className="priority-header">
      <div className="card-heading">
        <span className="card-icon"><Pin size={18} aria-hidden="true" /></span>
        <div>
          <h2 id="priority-heading">Ближайшие цели</h2>
          {!goals.length && <p>{canChoose ? 'Выберите до трёх целей из своих занятий.' : 'Добавьте цель к занятию, чтобы видеть её здесь.'}</p>}
        </div>
      </div>
      <button className={goals.length ? 'button secondary priority-manage' : 'button primary'}
        disabled={busy} onClick={event => manage(event.currentTarget)}>{goals.length ? 'Изменить' : <><Plus size={16} />{canChoose ? 'Выбрать цели' : 'Добавить первую цель'}</>}</button>
    </div>
    {goals.length > 0 && <ol className="priority-cards" data-count={goals.length}>
      {goals.map((goal, index) => {
        const activity = activities.find(item => item.id === goal.activity)
        return <li key={keyOf(goal)} className="priority-card">
          <div className="priority-card-meta">
            <span className="priority-rank" aria-label={'Приоритет ' + (index + 1)}>{index + 1}</span>
            <span className="priority-activity"><ActivityMark activity={activity} /><span>{activity?.name}</span></span>
          </div>
          <strong>{goal.goal}</strong>
          {!sessionActive && <button className="priority-prepare" disabled={busy} onClick={() => prepare(goal.activity)}
            aria-label={'Подготовить сессию для цели «' + goal.goal + '»'}>Подготовить сессию <ArrowRight size={14} /></button>}
        </li>
      })}
    </ol>}
  </section>
}

export function PriorityEditor({ initial, activities, activityId, focusGoal, busy, error, close, save, addGoal }: {
  initial: PriorityGoal[]; activities: Activity[]; activityId?: number; focusGoal?: string; busy: boolean; error: string
  close: () => void; save: (goals: PriorityGoal[]) => void; addGoal: (activity?: Activity, activityId?: number) => void
}) {
  const [selected, setSelected] = useState(initial)
  const [query, setQuery] = useState('')
  const [scope, setScope] = useState(activityId)
  const [replacement, setReplacement] = useState<PriorityGoal>()
  const search = useRef<HTMLInputElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  const initialAction = useRef<HTMLButtonElement>(null)
  const saveButton = useRef<HTMLButtonElement>(null)
  const replaceFirst = useRef<HTMLButtonElement>(null)
  const activity = activities.find(item => item.id === scope)
  const replacementActivity = activities.find(item => item.id === replacement?.activity)
  const available = activities.filter(hasGoals)
  const scoped = activity ? available.filter(item => item.id === activity.id) : available
  const searchText = query.trim().toLocaleLowerCase('ru')
  const groups = scoped.map(item => ({
    activity: item, goals: (item.goals ?? []).filter(goal =>
      item.name.toLocaleLowerCase('ru').includes(searchText) || goal.toLocaleLowerCase('ru').includes(searchText)),
  })).filter(group => group.goals.length)
  const full = selected.length >= priorityGoalLimit
  const dirty = selected.length !== initial.length || selected.some((goal, index) => !samePriorityGoal(goal, initial[index]))
  const firstGoal = !available.length && !activity && !selected.length && !dirty
  const activeActivities = firstGoal ? activities.filter(item => !item.archived) : []

  useEffect(() => {
    const frame = requestAnimationFrame(() => (initialAction.current ?? search.current ?? cancel.current)?.focus())
    return () => cancelAnimationFrame(frame)
  }, [])

  function move(index: number, step: number) {
    const next = [...selected]
    const target = index + step
    if (target < 0 || target >= next.length) return
    next.splice(target, 0, ...next.splice(index, 1))
    setSelected(next)
  }

  function remove(goal: PriorityGoal) {
    setSelected(selected.filter(item => !samePriorityGoal(item, goal)))
    const nextFocus = search.current ?? cancel.current
    nextFocus?.focus()
  }

  function cancelReplacement() {
    setReplacement(undefined)
    requestAnimationFrame(() => search.current?.focus())
  }

  return <form className="priority-editor" onSubmit={event => {
    event.preventDefault()
    if (!busy && dirty && !replacement) save(selected)
  }} onKeyDown={event => {
    if (event.key === 'Escape' && replacement) {
      event.preventDefault()
      cancelReplacement()
    }
  }}>
    <div className="dialog-heading">
      <span className="card-icon"><Pin size={18} aria-hidden="true" /></span>
      <div><h2>{replacement ? 'Заменить ближайшую цель' : firstGoal ? 'Добавить первую цель' : 'Ближайшие цели'}</h2>
        <p>{replacement ? 'Новая цель появится на выбранной позиции.' : firstGoal ? 'После сохранения цель можно закрепить на главной.' : 'На главной первой будет самая важная цель.'}</p></div>
    </div>
    <div className="dialog-body">
      {replacement ? <section className="priority-replacement">
        <div className="priority-selection"><h3>Новая цель</h3>
          <div className="priority-selected-label"><strong>{replacement.goal}</strong><small>{replacementActivity?.name}</small></div>
        </div>
        <h3>Какую цель заменить?</h3>
        <ol className="priority-choice-list">{selected.map((goal, index) => <li key={keyOf(goal)}>
          <button type="button" className="priority-option" ref={index === 0 ? replaceFirst : undefined} onClick={() => {
            setSelected(selected.map((item, position) => position === index ? replacement : item))
            setReplacement(undefined)
            requestAnimationFrame(() => saveButton.current?.focus())
          }}>
            <span className="priority-rank">{index + 1}</span>
            <span className="priority-selected-label"><strong>{goal.goal}</strong><small>{activities.find(item => item.id === goal.activity)?.name}</small></span>
            <span className="priority-choice-action">Заменить</span>
          </button>
        </li>)}</ol>
      </section> : firstGoal ? <section className="priority-first-goal">
        {activeActivities.length > 0 && <>
          <h3>Выберите занятие</h3>
          <ul className="priority-choice-list">{activeActivities.map((item, index) => <li key={item.id}>
            <button ref={index === 0 ? initialAction : undefined} type="button" className="priority-option" disabled={busy} onClick={() => addGoal(item)}>
              <ActivityMark activity={item} /><span className="priority-choice-name">{item.name}</span><ArrowRight size={15} aria-hidden="true" />
            </button>
          </li>)}</ul>
        </>}
        {!activeActivities.length && <button ref={initialAction} type="button" className="button primary" disabled={busy} onClick={() => addGoal()}>
          <Plus size={16} />Создать занятие с целью
        </button>}
      </section> : <>
      <section className="priority-selection" aria-labelledby="priority-selection-heading">
        <div className="priority-selection-header">
          <h3 id="priority-selection-heading">На главной</h3>
          <output aria-label="Выбрано целей">{selected.length} из {priorityGoalLimit}</output>
        </div>
        {selected.length ? <ol className="priority-selected">
          {selected.map((goal, index) => <li key={keyOf(goal)}>
            <span className="priority-rank" aria-hidden="true">{index + 1}</span>
            <div className="priority-selected-label"><strong>{goal.goal}</strong><small>{activities.find(item => item.id === goal.activity)?.name}</small></div>
            <div className="priority-order">
              <button type="button" className="icon-button" title="Выше" aria-label={'Переместить выше «' + goal.goal + '»'} disabled={busy}
                aria-disabled={index === 0} onClick={() => move(index, -1)}><ArrowUp size={15} /></button>
              <button type="button" className="icon-button" title="Ниже" aria-label={'Переместить ниже «' + goal.goal + '»'} disabled={busy}
                aria-disabled={index === selected.length - 1} onClick={() => move(index, 1)}><ArrowDown size={15} /></button>
              <button type="button" className="icon-button" title="Убрать с главной" aria-label={'Убрать с главной «' + goal.goal + '»'}
                disabled={busy} onClick={() => remove(goal)}><X size={15} /></button>
            </div>
          </li>)}
        </ol> : <p>{scoped.length ? 'Выберите цели из списка ниже.' : 'Пока ничего не закреплено.'}</p>}
      </section>
      <div className="priority-options-header">
        <h3>{activity ? `Цели занятия «${activity.name}»` : 'Цели занятий'}</h3>
        {activity && <button type="button" className="button secondary" disabled={busy} onClick={() => {
          setScope(undefined)
          setQuery('')
          requestAnimationFrame(() => (search.current ?? cancel.current)?.focus())
        }}>Все занятия</button>}
      </div>
      {scoped.length > 0 && <label className="activity-search">
        <Search size={16} aria-hidden="true" /><input ref={search} aria-label="Поиск целей и занятий"
          placeholder="Найти цель или занятие…" disabled={busy} value={query} onChange={event => setQuery(event.target.value)} />
      </label>}
      {full && scoped.length > 0 && <p className="priority-limit">Выберите другую цель, чтобы заменить одну из трёх.</p>}
      {groups.map(group => <section key={group.activity.id} className="priority-group">
        {!activity && <h4><ActivityMark activity={group.activity} />{group.activity.name}</h4>}
        <ul>
          {group.goals.map(goal => {
            const entry = { activity: group.activity.id, goal }
            const pinned = selected.some(item => samePriorityGoal(item, entry))
            return <li key={goal}><button type="button" className="priority-option" aria-pressed={pinned}
              ref={scope === activityId && goal === focusGoal ? initialAction : undefined}
              disabled={busy} title={pinned ? 'Убрать с главной' : full ? 'Заменить ближайшую цель' : 'Добавить на главную'} onClick={() => {
                if (pinned) setSelected(selected.filter(item => !samePriorityGoal(item, entry)))
                else if (full) {
                  setReplacement(entry)
                  requestAnimationFrame(() => replaceFirst.current?.focus())
                } else setSelected([...selected, entry])
              }}><span>{goal}</span>{pinned ? <Check size={17} /> : full ? <span className="priority-choice-action"><ArrowLeftRight size={15} />Заменить</span> : <Plus size={17} />}</button></li>
          })}
        </ul>
      </section>)}
      {!groups.length && <div className="priority-no-options">
        {scoped.length ? <p>По этому запросу целей нет.</p> : activity && !activity.archived && <>
          <p>У этого занятия пока нет активных целей.</p>
          {dirty ? <p>Сначала сохраните изменения в ближайших целях.</p> : <button ref={initialAction} type="button" className="button secondary" disabled={busy}
            onClick={() => addGoal(activity, scope)}>Добавить цель к занятию <ArrowRight size={15} /></button>}
        </>}
      </div>}
      </>}
      {error && <p className="error" role="alert">{error}</p>}
    </div>
    <div className="dialog-actions">
      <button ref={cancel} type="button" className="button secondary" disabled={busy} onClick={replacement ? cancelReplacement : close}>
        {replacement ? 'Назад' : firstGoal ? 'Закрыть' : 'Отмена'}
      </button>
      {!replacement && !firstGoal && <button ref={saveButton} className="button primary" disabled={busy || !dirty}>Сохранить</button>}
    </div>
  </form>
}
