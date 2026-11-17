import { useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import { Download, Link, Search, ListVideo, RefreshCw, ExternalLink } from 'lucide-react';
import { call } from '../bridge';
import { itemKey, useStore } from '../store';
import type { EnqueueResult, MediaItem, Source } from '../types';
import { duration } from '../format';
import { Empty, ErrorBox, Spinner, Status, Thumb } from '../components/Common';
import { Options } from '../components/Options';
export default function NewDownload() {
  const { t } = useTranslation();
  const { sources, sourceId, selected, options, destination, jobs } = useStore();
  const source = sources.find((s) => s.id === sourceId);
  const [urls, setUrls] = useState('');
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [adding, setAdding] = useState(false);
  const parent = useRef<HTMLDivElement>(null);
  const entries = source?.entries;
  const filtered = useMemo(
    () =>
      (entries || []).filter((i) =>
        i.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      ),
    [entries, query],
  );
  const virtual = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parent.current,
    estimateSize: () => 78,
    overscan: 8,
    getItemKey: (i) => itemKey(filtered[i]),
  });
  const jobMap = useMemo(() => {
    const map = new Map<string, string>();
    for (const j of [...jobs].reverse()) map.set(itemKey(j.item), j.status);
    return map;
  }, [jobs]);
  const allSelected = filtered.length > 0 && filtered.every((i) => selected.has(itemKey(i)));
  const chosen = (entries || []).filter((i) => selected.has(itemKey(i)));
  async function resolve(refresh = false) {
    setBusy(true);
    try {
      const inputs = refresh
        ? source?.urls
        : urls
            .split(/\r?\n/)
            .map((s) => s.trim())
            .filter(Boolean);
      const result = await call<Source>('resolve_source', { urls: inputs || [] });
      useStore.getState().setSource(result.id);
      setQuery('');
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setBusy(false);
    }
  }
  async function add(items: MediaItem[]) {
    setAdding(true);
    try {
      const result = await call<EnqueueResult>('enqueue', {
        request: { items, options, destination, sourceTitle: source?.title || 'Videos' },
      });
      for (const j of result.jobs) useStore.getState().event({ type: 'job', data: j });
      useStore
        .getState()
        .setNotice(t('enqueued', { count: result.jobs.length, skipped: result.skipped }));
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setAdding(false);
    }
  }
  return (
    <div className="split">
      <section className="new-content">
        <div className="url-section">
          <h1>{t('urlTitle')}</h1>
          <p>{t('urlHint')}</p>
          <form
            className="url-form"
            onSubmit={(e) => {
              e.preventDefault();
              void resolve();
            }}
          >
            <div className="url-input">
              <Link size={17} />
              <textarea
                aria-label={t('urlPlaceholder')}
                placeholder="https://www.youtube.com/@channel/shorts"
                rows={1}
                value={urls}
                onChange={(e) => setUrls(e.target.value)}
              />
            </div>
            <button
              className="primary"
              disabled={!urls.trim() || busy || source?.status === 'loading'}
            >
              {busy ? <Spinner /> : null}
              {t('getList')}
            </button>
          </form>
        </div>
        {source ? (
          <>
            <div className="source-heading">
              <div className="source-symbol">
                <ListVideo size={23} />
              </div>
              <div className="source-name">
                <h2>{source.title || source.urls[0]}</h2>
                <p>
                  {t('listCount', { count: source.entries.length })} · {t(source.status)}
                </p>
              </div>
              {source.status === 'loading' ? (
                <button
                  className="text-button"
                  onClick={() =>
                    void call('cancel_source', { id: source.id }).catch(
                      useStore.getState().setError,
                    )
                  }
                >
                  {t('cancelParsing')}
                </button>
              ) : (
                <button className="text-button" onClick={() => void resolve(true)} disabled={busy}>
                  <RefreshCw size={14} />
                  {t('refresh')}
                </button>
              )}
            </div>
            {source.error && <ErrorBox error={source.error} />}
            <div className="selection-toolbar">
              <label className="check">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={(e) =>
                    useStore.getState().selectMany(filtered.map(itemKey), e.target.checked)
                  }
                />
                <span>{t('allResults')}</span>
              </label>
              <span className="selection-count">{t('selected', { count: chosen.length })}</span>
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
            <div className="video-table-head">
              <span />
              <span>{t('video')}</span>
              <span>{t('duration')}</span>
              <span>{t('status')}</span>
              <span>{t('action')}</span>
            </div>
            <div className="video-scroll" ref={parent}>
              {!filtered.length ? (
                <Empty
                  icon={source.status === 'loading' ? <Spinner /> : <Search size={28} />}
                  heading={t(source.status === 'loading' ? 'parsing' : 'noResults')}
                />
              ) : (
                <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
                  {virtual.getVirtualItems().map((row) => {
                    const item = filtered[row.index];
                    const key = itemKey(item);
                    const status = jobMap.get(key) || 'pending';
                    return (
                      <div
                        className="video-row"
                        key={key}
                        style={{
                          position: 'absolute',
                          width: '100%',
                          height: row.size,
                          transform: `translateY(${row.start}px)`,
                        }}
                      >
                        <input
                          type="checkbox"
                          aria-label={item.title}
                          checked={selected.has(key)}
                          onChange={(e) => useStore.getState().select(key, e.target.checked)}
                        />
                        <div className="video-cell">
                          <Thumb item={item} />
                          <div className="video-label">
                            <span title={item.title}>{item.title}</span>
                            <small>{item.uploader || item.site}</small>
                          </div>
                        </div>
                        <span className="muted tabular">{duration(item.duration)}</span>
                        <Status value={status} />
                        <button
                          className="text-button"
                          onClick={() => void add([item])}
                          disabled={adding}
                        >
                          {t('download')}
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
            <footer className="list-footer">
              <span>{t('listCount', { count: source.entries.length })}</span>
              <button
                className="text-button"
                onClick={() =>
                  void call('open_url', { url: source.urls[0] }).catch(useStore.getState().setError)
                }
              >
                <ExternalLink size={12} />
                {t('openOriginal')}
              </button>
            </footer>
          </>
        ) : (
          <Empty
            icon={<ListVideo size={32} />}
            heading={t('emptyList')}
            detail={t('emptyListHint')}
          />
        )}
      </section>
      <aside className="inspector">
        <h2>{t('downloadSettings')}</h2>
        <Options
          options={options}
          onChange={useStore.getState().setOptions}
          destination={destination}
          onDestination={useStore.getState().setDestination}
        />
        <div className="inspector-actions">
          <small>{t('selected', { count: chosen.length })}</small>
          <button
            className="primary"
            disabled={!chosen.length || adding}
            onClick={() => void add(chosen)}
          >
            <Download size={16} />
            {t('downloadSelected', { count: chosen.length })}
          </button>
          <button
            className="text-button"
            disabled={!filtered.length || adding}
            onClick={() => void add(filtered)}
          >
            {t('downloadAll')}
          </button>
        </div>
      </aside>
    </div>
  );
}
