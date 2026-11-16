import { useState } from 'react'
import {
  AlarmClock, Anchor, Award, BatteryCharging, BookOpen, Brain, CalendarCheck, CalendarDays,
  Check, ChevronDown, ChevronRight, CircleCheck, Clock3, Compass, Crown, Feather, Flag, Flame,
  Footprints, Gamepad2, Gem, HeartHandshake, Hourglass, Layers3, LockKeyhole, Medal, Moon,
  Mountain, Music2, NotebookPen, Orbit, Rocket, RotateCcw, Scale, Search, Shield, ShieldCheck,
  Sparkles, Star, Sunrise, Swords, Target, Timer, TimerReset, Trophy, X, Zap,
  type LucideIcon,
} from 'lucide-react'
import { duration, type Snapshot } from './types'

const groups = [
  { id: 'start', label: 'Первые шаги', icon: Compass },
  { id: 'discipline', label: 'Дисциплина', icon: Flame },
  { id: 'time', label: 'Время — золото', icon: Hourglass },
  { id: 'marathon', label: 'Марафонец', icon: Mountain },
  { id: 'projects', label: 'Проекты и развитие', icon: Layers3 },
  { id: 'balance', label: 'Баланс и игры', icon: Gamepad2 },
  { id: 'secret', label: 'Секретные', icon: Sparkles },
] as const
const rarityLabels = { common: 'Обычное', rare: 'Редкое', epic: 'Эпическое', legendary: 'Легендарное' } as const
type Rarity = keyof typeof rarityLabels
type Group = typeof groups[number]['id']
type Unit = 'event' | 'goal' | 'day' | 'time' | 'elapsedDays' | 'activity'

type MedalDefinition = {
  id: string
  name: string
  description: string
  group: Group
  rarity: Rarity
  icon: LucideIcon
  unit: Unit
}

const catalog: MedalDefinition[] = [
  { id: 'first-step', name: 'Первый шаг', description: 'Завершите первую рабочую сессию.', group: 'start', rarity: 'common', icon: Footprints, unit: 'event' },
  { id: 'warm-up', name: 'Разминка окончена', description: 'Накопите 45 минут работы.', group: 'start', rarity: 'common', icon: BatteryCharging, unit: 'time' },
  { id: 'balanced-day', name: 'День в балансе', description: 'Выполните оба этапа за один день.', group: 'start', rarity: 'common', icon: CalendarCheck, unit: 'goal' },
  { id: 'right-track', name: 'На правильном пути', description: 'Выполните дневную цель 5 раз.', group: 'start', rarity: 'rare', icon: Compass, unit: 'goal' },
  { id: 'good-rhythm', name: 'Хороший ритм', description: 'Выполните дневную цель 7 раз.', group: 'start', rarity: 'rare', icon: Music2, unit: 'goal' },
  { id: 'new-habit', name: 'Новая привычка', description: 'Выполните дневную цель 30 раз.', group: 'start', rarity: 'epic', icon: CalendarDays, unit: 'goal' },
  { id: 'streak-3', name: 'Три дня свободы', description: 'Выполняйте цель 3 дня подряд.', group: 'discipline', rarity: 'common', icon: Flag, unit: 'day' },
  { id: 'streak-7', name: 'Неделя силы', description: 'Выполняйте цель 7 дней подряд.', group: 'discipline', rarity: 'rare', icon: Flame, unit: 'day' },
  { id: 'streak-14', name: 'Железная воля', description: 'Выполняйте цель 14 дней подряд.', group: 'discipline', rarity: 'epic', icon: Shield, unit: 'day' },
  { id: 'streak-30', name: 'Несокрушимый', description: 'Выполняйте цель 30 дней подряд.', group: 'discipline', rarity: 'epic', icon: Mountain, unit: 'day' },
  { id: 'streak-100', name: 'Легенда дисциплины', description: 'Выполняйте цель 100 дней подряд.', group: 'discipline', rarity: 'legendary', icon: Swords, unit: 'day' },
  { id: 'hours-1', name: 'Первый час', description: 'Накопите 1 час работы.', group: 'time', rarity: 'common', icon: Clock3, unit: 'time' },
  { id: 'hours-10', name: 'Десяточка', description: 'Накопите 10 часов работы.', group: 'time', rarity: 'common', icon: Timer, unit: 'time' },
  { id: 'hours-50', name: 'Серьёзный подход', description: 'Накопите 50 часов работы.', group: 'time', rarity: 'rare', icon: Hourglass, unit: 'time' },
  { id: 'hours-100', name: 'Сотня', description: 'Накопите 100 часов работы.', group: 'time', rarity: 'epic', icon: Medal, unit: 'time' },
  { id: 'hours-500', name: 'Мастер времени', description: 'Накопите 500 часов работы.', group: 'time', rarity: 'legendary', icon: Gem, unit: 'time' },
  { id: 'hours-1000', name: 'Повелитель времени', description: 'Накопите 1000 часов работы.', group: 'time', rarity: 'legendary', icon: Orbit, unit: 'time' },
  { id: 'flow', name: 'Поймал поток', description: 'Наберите 90 минут в одной сессии.', group: 'marathon', rarity: 'rare', icon: Zap, unit: 'time' },
  { id: 'deep-work', name: 'Глубокая работа', description: 'Проработайте 2 часа без паузы.', group: 'marathon', rarity: 'rare', icon: Brain, unit: 'time' },
  { id: 'one-breath', name: 'На одном дыхании', description: 'Проработайте 3 часа без паузы.', group: 'marathon', rarity: 'epic', icon: Feather, unit: 'time' },
  { id: 'marathon', name: 'Марафон', description: 'Проработайте 5 часов за один день.', group: 'marathon', rarity: 'epic', icon: Rocket, unit: 'time' },
  { id: 'unstoppable', name: 'Неудержимый', description: 'Проработайте 8 часов за один день.', group: 'marathon', rarity: 'legendary', icon: Crown, unit: 'time' },
  { id: 'new-start', name: 'Новое начало', description: 'Создайте своё первое занятие.', group: 'projects', rarity: 'common', icon: BookOpen, unit: 'event' },
  { id: 'multitask', name: 'Многозадачность', description: 'Поработайте над 3 занятиями.', group: 'projects', rarity: 'common', icon: Layers3, unit: 'activity' },
  { id: 'orchestra', name: 'Человек-оркестр', description: 'Поработайте над 10 занятиями.', group: 'projects', rarity: 'rare', icon: Star, unit: 'activity' },
  { id: 'back-to-work', name: 'Вернулся к делу', description: 'Вернитесь к занятию после 7 дней перерыва.', group: 'projects', rarity: 'rare', icon: RotateCcw, unit: 'elapsedDays' },
  { id: 'craft-master', name: 'Мастер своего дела', description: 'Накопите 100 часов в одном занятии.', group: 'projects', rarity: 'epic', icon: Award, unit: 'time' },
  { id: 'result', name: 'Есть результат!', description: 'Впервые сохраните заметку о результате сессии.', group: 'projects', rarity: 'common', icon: NotebookPen, unit: 'event' },
  { id: 'work-first', name: 'Сначала дело', description: 'Завершите первый этап до первого запуска игры за день.', group: 'balance', rarity: 'rare', icon: ShieldCheck, unit: 'event' },
  { id: 'early-freedom', name: 'Ранняя свобода', description: 'Накопите 90 минут работы до 16:00.', group: 'balance', rarity: 'rare', icon: AlarmClock, unit: 'time' },
  { id: 'perfect-balance', name: 'Идеальный баланс', description: 'За завершившийся день поработайте дольше, чем играли.', group: 'balance', rarity: 'rare', icon: Scale, unit: 'event' },
  { id: 'mindful-player', name: 'Осознанный игрок', description: 'Выполните дневную цель и поиграйте в один и тот же день 7 раз.', group: 'balance', rarity: 'epic', icon: HeartHandshake, unit: 'day' },
  { id: 'games-can-wait', name: 'Игры подождут', description: 'Выполните дневную цель до начала игр.', group: 'balance', rarity: 'rare', icon: LockKeyhole, unit: 'event' },
  { id: 'night-owl', name: 'Полуночник', description: 'Завершите рабочую сессию между 00:00 и 06:00.', group: 'secret', rarity: 'rare', icon: Moon, unit: 'event' },
  { id: 'early-bird', name: 'Ранняя пташка', description: 'Начните рабочую сессию между 06:00 и 08:00.', group: 'secret', rarity: 'rare', icon: Sunrise, unit: 'event' },
  { id: 'five-more', name: 'Ещё пять минуточек', description: 'Проработайте 30 минут сверх дневной цели.', group: 'secret', rarity: 'rare', icon: TimerReset, unit: 'time' },
  { id: 'hero-returns', name: 'Возвращение героя', description: 'Выполните дневную цель после 14 дней без выполненных целей.', group: 'secret', rarity: 'epic', icon: Anchor, unit: 'day' },
  { id: 'way-of-life', name: 'Это уже образ жизни', description: 'Выполните 365 дневных целей.', group: 'secret', rarity: 'legendary', icon: Trophy, unit: 'goal' },
]

const awardDate = new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium' })
const visibleCatalog = (awards: Snapshot['awards']) => catalog.filter(item => item.group !== 'secret' || awards[item.id] !== undefined)

function Badge({ item, earned, small = false }: { item: MedalDefinition; earned: boolean; small?: boolean }) {
  const Icon = item.icon
  return <span className={'medal-badge' + (earned ? ' earned' : '') + (small ? ' small' : '')} data-rarity={item.rarity} aria-hidden="true">
    <span><Icon size={small ? 17 : 26} strokeWidth={1.75} /></span>
  </span>
}

function progressText(unit: Unit, current: number, target: number) {
  if (unit === 'time') return duration(current) + ' / ' + duration(target)
  if (unit === 'elapsedDays') return Math.floor(current / 86400) + ' / ' + Math.floor(target / 86400) + ' дн.'
  return current + ' / ' + target + (unit === 'goal' ? ' целей' : unit === 'day' ? ' дн.' : unit === 'activity' ? ' занятий' : '')
}

export function Achievements({ data }: { data: Snapshot }) {
  const [group, setGroup] = useState<Group | 'all'>('all')
  const [status, setStatus] = useState<'all' | 'earned' | 'locked'>('all')
  const [rarity, setRarity] = useState<Rarity | 'all'>('all')
  const [query, setQuery] = useState('')
  const available = visibleCatalog(data.awards)
  const earned = available.filter(item => data.awards[item.id] !== undefined).length
  const percent = Math.round(earned / available.length * 100)
  const input = query.trim().toLocaleLowerCase('ru')
  const filtered = available.filter(item => {
    const unlocked = data.awards[item.id] !== undefined
    return (group === 'all' || item.group === group) && (rarity === 'all' || item.rarity === rarity)
      && (status === 'all' || (status === 'earned') === unlocked)
      && (item.name + ' ' + item.description).toLocaleLowerCase('ru').includes(input)
  })
  return <div className="page awards-page">
    <div className="heading"><div><h1>Достижения</h1><p>Награды за фокус, регулярность и баланс с играми.</p></div></div>
    <section className="panel awards-hero" aria-label="Прогресс коллекции">
      <div className="awards-hero-copy"><span className="card-icon"><Trophy size={20} /></span><div><h2>Коллекция наград</h2><p><strong>{earned}</strong> из {available.length} получено</p></div></div>
      <div className="award-ring" style={{ background: 'conic-gradient(#c09bff ' + percent + '%, #392c4e 0)' }}><span>{percent}%</span></div>
    </section>
    <div className="award-filters">
      <div className="award-categories" role="group" aria-label="Категория достижений">
        <button aria-pressed={group === 'all'} onClick={() => setGroup('all')}><Medal size={15} />Все</button>
        {groups.filter(item => item.id !== 'secret' || available.some(medal => medal.group === 'secret')).map(({ id, label, icon: Icon }) =>
          <button key={id} aria-pressed={group === id} onClick={() => setGroup(id)}><Icon size={15} />{label}</button>)}
      </div>
      <div className="award-filter-row">
        <div className="activity-tabs" role="group" aria-label="Статус достижения">
          <button aria-pressed={status === 'all'} onClick={() => setStatus('all')}>Все</button>
          <button aria-pressed={status === 'earned'} onClick={() => setStatus('earned')}>Полученные</button>
          <button aria-pressed={status === 'locked'} onClick={() => setStatus('locked')}>В процессе</button>
        </div>
        <label className="award-rarity-select"><select aria-label="Редкость" value={rarity} onChange={event => setRarity(event.target.value as Rarity | 'all')}><option value="all">Любая редкость</option>{Object.entries(rarityLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select><ChevronDown size={14} /></label>
        <label className="activity-search award-search"><Search size={15} /><input aria-label="Поиск достижений" value={query} placeholder="Найти достижение…" onChange={event => setQuery(event.target.value)} /></label>
      </div>
    </div>
    {!data.achievementProgress ? <div className="loading">Загрузка достижений…</div> : filtered.length ? groups.map(({ id, label, icon: Icon }) => {
      const items = filtered.filter(item => item.group === id)
      if (!items.length) return null
      return <section className="award-section" key={id}>
        <div className="award-section-title"><Icon size={19} /><h2>{label}</h2><small>{items.length}</small></div>
        <div className="award-grid">{items.map(item => {
          const timestamp = data.awards[item.id]
          const unlocked = timestamp !== undefined
          const progress = data.achievementProgress?.[item.id]
          const target = progress?.target ?? 1
          const current = unlocked ? target : progress?.current ?? 0
          return <article key={item.id} className={'panel award-card' + (unlocked ? ' unlocked' : '')} data-rarity={item.rarity} title={unlocked ? 'Начислено ' + awardDate.format(timestamp * 1000) : undefined}>
            <div className="award-card-top"><Badge item={item} earned={unlocked} /><div><span className="award-rarity">{rarityLabels[item.rarity]}</span><h3>{item.name}</h3></div>{unlocked && <CircleCheck className="award-check" size={17} aria-label="Получено" />}</div>
            <p>{item.description}</p>
            <div className="award-progress-label"><span>{unlocked ? 'Получено' : 'Прогресс'}</span><span>{progressText(item.unit, current, target)}</span></div>
            <div className="award-progress" role="progressbar" aria-label={item.name} aria-valuemin={0} aria-valuemax={target} aria-valuenow={current}><span style={{ width: current / target * 100 + '%' }} /></div>
          </article>
        })}</div>
      </section>
    }) : <section className="panel empty"><Search size={26} /><h3>Достижения не найдены</h3><p>Измените фильтры или поисковый запрос.</p><button className="button secondary" onClick={() => { setGroup('all'); setStatus('all'); setRarity('all'); setQuery('') }}>Сбросить фильтры</button></section>}
  </div>
}

export function AchievementPreview({ awards, open }: { awards: Snapshot['awards']; open: () => void }) {
  const available = visibleCatalog(awards)
  const earned = available.filter(item => awards[item.id] !== undefined).sort((a, b) => awards[b.id] - awards[a.id])
  const items = earned.length > 0 ? earned.slice(0, 3) : available.slice(0, 3)
  return <section className="panel list-panel">
    <div className="card-heading"><span className="card-icon"><Trophy size={19} /></span><div><h2>Достижения</h2><p>{earned.length} из {available.length} получено</p></div></div>
    <div className="achievements">{items.map(item => <div className="achievement" key={item.id}><Badge item={item} earned={awards[item.id] !== undefined} small /><div><b>{item.name}</b><small>{item.description}</small></div>{awards[item.id] !== undefined && <Check className="preview-earned" size={17} />}</div>)}</div>
    <button className="button secondary award-catalog-link" onClick={open}>Открыть каталог <ChevronRight size={15} /></button>
  </section>
}

export function AchievementUnlock({ ids, close, open }: { ids: string[]; close: () => void; open: () => void }) {
  const item = catalog.find(medal => medal.id === ids[0])
  if (!item) return null
  return <div className="award-unlock" data-rarity={item.rarity} role="status">
    <Badge item={item} earned />
    <div><small>{ids.length === 1 ? 'Достижение получено' : 'Новых достижений: ' + ids.length}</small><strong>{item.name}</strong><button onClick={open}>Открыть каталог <ChevronRight size={13} /></button></div>
    <button className="award-unlock-close" aria-label="Закрыть уведомление" onClick={close}><X size={17} /></button>
  </div>
}
