import { useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import {
  Download,
  Pause,
  Play,
  RotateCw,
  X,
  FolderOpen,
  FileDown,
  Search,
  CheckCircle2,
} from 'lucide-react';
import { activeStatuses, useStore } from '../store';
import { call, chooseCsv } from '../bridge';
import type { Job } from '../types';
import { bytes, duration, timestamp } from '../format';
import { Empty, Status, Thumb } from '../components/Common';
export default function Downloads({ completed = false }: { completed?: boolean }) {
  const { t } = useTranslation();
  const jobs = useStore((s) => s.jobs);
  const [filter, setFilter] = useState('all');
  const [query, setQuery] = useState('');
  const [working, setWorking] = useState(false);
  const parent = useRef<HTMLDivElement>(null);
  const filtered = useMemo(
    () =>
      jobs.filter(
        (j) =>
          (completed ? j.status === 'completed' : j.status !== 'completed') &&
          (filter === 'all' ||
            (filter === 'active' ? activeStatuses.has(j.status) : j.status === filter)) &&
          j.item.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      ),
    [jobs, completed, filter, query],
  );
  const virtual = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parent.current,
    estimateSize: () => (completed ? 92 : 132),
    overscan: 5,
    getItemKey: (i) => filtered[i].id,
  });
  async function control(ids: string[], action: string) {
    setWorking(true);
    try {
      const updated = await call<Job[]>('control_jobs', { ids, action });
      for (const j of updated) useStore.getState().event({ type: 'job', data: j });
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setWorking(false);
    }
  }
  async function exportCsv() {
    try {
      const path = await chooseCsv();
      if (path) {
        const count = await call<number>('export_csv', { path });
        useStore.getState().setNotice(t('exported', { count }));
      }
    } catch (e) {
      useStore.getState().setError(e);
    }
  }
  const action = (command: string, id: string) =>
    void call(command, { id }).catch(useStore.getState().setError);
  return (
    <section className="page downloads-page">
      <div className="page-title">
        <div>
          <h1>{t(completed ? 'completedTitle' : 'queueTitle')}</h1>
          <p>{t(completed ? 'completedHint' : 'queueHint')}</p>
        </div>
        {completed ? (
          <button disabled={!filtered.length} onClick={() => void exportCsv()}>
            <FileDown size={16} />
            {t('export')}
          </button>
        ) : (
          <div className="button-group">
            <button
              disabled={working || !jobs.some((j) => activeStatuses.has(j.status))}
              onClick={() =>
                void control(
                  jobs.filter((j) => activeStatuses.has(j.status)).map((j) => j.id),
                  'pause',
                )
              }
            >
              <Pause size={14} />
              {t('pauseAll')}
            </button>
            <button
              disabled={working || !jobs.some((j) => j.status === 'paused')}
              onClick={() =>
                void control(
                  jobs.filter((j) => j.status === 'paused').map((j) => j.id),
                  'resume',
                )
              }
            >
              <Play size={14} />
              {t('resumeAll')}
            </button>
            <button
              disabled={working || !jobs.some((j) => j.status === 'failed')}
              onClick={() =>
                void control(
                  jobs.filter((j) => j.status === 'failed').map((j) => j.id),
                  'retry',
                )
              }
            >
              <RotateCw size={14} />
              {t('retryFailed')}
            </button>
          </div>
        )}
      </div>
      <div className="queue-toolbar">
        {!completed && (
          <div className="tabs">
            {['all', 'active', 'paused', 'failed'].map((f) => (
              <button
                key={f}
                className={filter === f ? 'selected' : ''}
                onClick={() => setFilter(f)}
              >
                {t(f)}
              </button>
            ))}
          </div>
        )}
        <span className="muted">{t('totalJobs', { count: filtered.length })}</span>
        <div className="search">
          <Search size={15} />
          <input
            aria-label={t('searchTitle')}
            placeholder={t('searchTitle')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>
      <div ref={parent} className="jobs-scroll">
        {!filtered.length ? (
          <Empty
            icon={completed ? <CheckCircle2 size={32} /> : <Download size={32} />}
            heading={t(completed ? 'emptyCompleted' : 'emptyQueue')}
            detail={t(completed ? 'emptyCompletedHint' : 'emptyQueueHint')}
          />
        ) : (
          <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {virtual.getVirtualItems().map((row) => {
              const j = filtered[row.index];
              return (
                <article
                  className={`job-row ${completed ? 'completed-row' : ''}`}
                  key={j.id}
                  style={{
                    position: 'absolute',
                    width: '100%',
                    height: row.size,
                    transform: `translateY(${row.start}px)`,
                  }}
                >
                  <Thumb item={j.item} />
                  <div className="job-main">
                    <div className="job-title">
                      <strong title={j.item.title}>{j.item.title}</strong>
                      <Status value={j.status} />
                    </div>
                    {completed ? (
                      <>
                        <p className="job-meta">
                          {bytes(j.fileSize)} · {duration(j.duration)} ·{' '}
                          {j.resolution || j.options.audioFormat.toUpperCase()}
                        </p>
                        <p className="file-path" title={j.filePath || ''}>
                          {j.filePath}
                        </p>
                      </>
                    ) : (
                      <>
                        <div
                          className={`progress ${j.progress === null && j.status === 'downloading' ? 'indeterminate' : ''}`}
                          role="progressbar"
                          aria-label={j.item.title}
                          aria-valuenow={j.progress ?? undefined}
                          aria-valuemin={0}
                          aria-valuemax={100}
                        >
                          <span style={{ width: `${j.progress ?? 0}%` }} />
                        </div>
                        <div className="job-meta">
                          <span>
                            {j.progress !== null ? `${j.progress.toFixed(1)}% · ` : ''}
                            {bytes(j.bytesDownloaded)} /{' '}
                            {j.bytesTotal ? bytes(j.bytesTotal) : t('bytesUnknown')}
                          </span>
                          <span>
                            {j.speed ? `${bytes(j.speed)}/s` : ''}
                            {j.eta ? ` · ${t('remaining', { time: duration(j.eta) })}` : ''}
                          </span>
                        </div>
                        <div className="job-note">
                          {j.error ? (
                            <details>
                              <summary>
                                {t(`errors.${j.error.code}`, {
                                  defaultValue: t('errors.download_failed'),
                                })}
                              </summary>
                              <pre>{j.error.detail}</pre>
                            </details>
                          ) : (
                            <span>
                              {j.item.uploader || j.item.site} ·{' '}
                              {t('attempt', { count: j.attempt })}
                            </span>
                          )}
                          {j.retryAt && j.status === 'retry_wait' && (
                            <span>{t('retryAt', { time: timestamp(j.retryAt) })}</span>
                          )}
                          {j.fallbackUsed && <span>{t('fallback')}</span>}
                        </div>
                      </>
                    )}
                  </div>
                  <div className="job-actions">
                    {completed ? (
                      <>
                        <button
                          title={t('play')}
                          aria-label={t('play')}
                          onClick={() => action('open_file', j.id)}
                        >
                          <Play size={16} />
                        </button>
                        <button
                          title={t('reveal')}
                          aria-label={t('reveal')}
                          onClick={() => action('reveal_file', j.id)}
                        >
                          <FolderOpen size={16} />
                        </button>
                      </>
                    ) : (
                      <>
                        {activeStatuses.has(j.status) ? (
                          <button
                            title={t('pause')}
                            aria-label={t('pause')}
                            disabled={working}
                            onClick={() => void control([j.id], 'pause')}
                          >
                            <Pause size={16} />
                          </button>
                        ) : (
                          <button
                            title={t(j.status === 'paused' ? 'resume' : 'retry')}
                            aria-label={t(j.status === 'paused' ? 'resume' : 'retry')}
                            disabled={working}
                            onClick={() =>
                              void control([j.id], j.status === 'paused' ? 'resume' : 'retry')
                            }
                          >
                            <Play size={16} />
                          </button>
                        )}
                        {j.status !== 'cancelled' && (
                          <button
                            title={t('cancel')}
                            aria-label={t('cancel')}
                            disabled={working}
                            onClick={() => void control([j.id], 'cancel')}
                          >
                            <X size={16} />
                          </button>
                        )}
                      </>
                    )}
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
