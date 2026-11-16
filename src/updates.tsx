import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Download } from 'lucide-react'
import { desktop, type UpdateStatus } from './desktop'

export function useUpdates() {
  const [status, setStatus] = useState<UpdateStatus>()
  const request = useRef(0)
  const refresh = useCallback(async () => {
    const id = ++request.current
    const next = await desktop.updateStatus()
    if (id === request.current) setStatus(next)
  }, [])

  useEffect(() => {
    const poll = () => { if (!document.hidden) void refresh().catch(() => {}) }
    poll()
    const interval = window.setInterval(poll, 1000)
    document.addEventListener('visibilitychange', poll)
    return () => {
      ++request.current
      window.clearInterval(interval)
      document.removeEventListener('visibilitychange', poll)
    }
  }, [refresh])

  return { status, refresh }
}

export function UpdateSettings({ status, busy, toggle, check, download, confirm }: {
  status?: UpdateStatus; busy: boolean; toggle: ReactNode
  check: () => void; download: () => void; confirm: () => void
}) {
  const phase = status?.phase
  const downloading = phase === 'downloading'
  const working = phase === 'checking' || downloading || phase === 'installing'
  const percent = status?.total ? Math.min(100, Math.floor(100 * status.received / status.total)) : undefined
  const message = {
    disabled: 'Обновления недоступны в этой сборке.',
    idle: '',
    checking: 'Проверяем GitHub Releases…',
    current: 'Установлена последняя версия.',
    available: `Доступна версия ${status?.version}.`,
    downloading: `Скачиваем версию ${status?.version}…${percent === undefined ? '' : ` ${percent}%`}`,
    ready: `Версия ${status?.version} скачана. Установите её при перезапуске.`,
    installing: 'Запускаем установщик. Ход установки покажет Windows.',
  }[phase ?? 'idle']

  return <section className="panel list-panel updates-panel">
    <div className="card-heading"><span className="card-icon"><Download size={19} /></span><div><h2>Обновления</h2><p>Текущая версия: {status?.currentVersion ?? '—'}</p></div></div>
    <div className="setting-row"><div><b>Автоматическое обновление</b><p>Скачивать новые версии в фоне. Установка начнётся после вашего подтверждения.</p></div>{toggle}</div>
    <div className="update-controls">
      {message && <p aria-live={downloading ? 'off' : 'polite'}>{message}</p>}
      {working && phase !== 'checking' && <progress aria-label={downloading ? 'Скачивание обновления' : 'Запуск установщика'} value={downloading ? percent : undefined} max={100} />}
      {status?.error && <p className="error" role="alert">{status.error}</p>}
      <div className="update-buttons">
        <button className="button" disabled={busy || !phase || phase === 'disabled' || working || phase === 'ready'} onClick={check}>Проверить обновления</button>
        {phase === 'available' && <button className="button primary" disabled={busy} onClick={download}>Обновить</button>}
        {phase === 'ready' && <button className="button primary" disabled={busy} onClick={confirm}>Перезапустить и установить</button>}
      </div>
    </div>
  </section>
}
