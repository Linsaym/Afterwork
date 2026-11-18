import { useEffect, useState } from 'react'
import { ChevronLeft } from 'lucide-react'

export function useHistoryQuery<T>(query: () => Promise<T>, revision: number | null) {
  const [result, setResult] = useState<{ query: typeof query; value?: T; error?: string }>()
  useEffect(() => {
    if (revision === null) return
    let current = true
    void query().then(value => {
      if (current) setResult({ query, value })
    }, error => {
      if (current) setResult({ query, error: String(error) })
    })
    return () => { current = false }
  }, [query, revision])
  return result?.query === query ? result : undefined
}

export function HistoryNavigation({ page, more, change, nextLabel = 'Ещё сессии' }: { page: number; more: boolean; change: (page: number) => void; nextLabel?: string }) {
  if (page === 0 && !more) return null
  return <div className="history-navigation">
    {page > 0 && <button className="button secondary" onClick={() => change(page - 1)}><ChevronLeft size={16} />Предыдущие</button>}
    {more && <button className="button secondary" onClick={() => change(page + 1)}>{nextLabel}</button>}
  </div>
}
