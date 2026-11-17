import { useTranslation } from 'react-i18next';
import { ArrowDownToLine, RefreshCw, X } from 'lucide-react';
import { bytes } from '../format';
import { useUpdates } from '../updates';
import { Spinner } from './Common';
export function UpdateDialog() {
  const { t } = useTranslation();
  const update = useUpdates();
  if (!update.visible || !update.info) return null;
  const busy = ['downloading', 'installing'].includes(update.status);
  return (
    <div className="modal-backdrop">
      <section
        className="update-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-title"
      >
        <div className="dialog-heading">
          <RefreshCw size={22} />
          <h2 id="update-title">{t('updateAvailable', { version: update.info.version })}</h2>
          {!busy && (
            <button className="icon-button" onClick={update.hide} aria-label={t('later')}>
              <X size={18} />
            </button>
          )}
        </div>
        <p className="hint">{t('updateHint')}</p>
        <h3>{t('releaseNotes')}</h3>
        <pre className="release-notes">{update.info.notes || '—'}</pre>
        {busy && (
          <>
            <p>{t(update.status === 'installing' ? 'updateInstalling' : 'updateDownloading')}</p>
            <div
              className={`progress ${!update.total ? 'indeterminate' : ''}`}
              role="progressbar"
              aria-label={t('updateDownloading')}
              aria-valuenow={
                update.total ? Math.min(100, (update.downloaded / update.total) * 100) : undefined
              }
            >
              <span
                style={{
                  width: update.total
                    ? `${Math.min(100, (update.downloaded / update.total) * 100)}%`
                    : '0%',
                }}
              />
            </div>
            <p className="hint">
              {bytes(update.downloaded)} / {bytes(update.total)}
            </p>
          </>
        )}
        {update.error && (
          <div className="error-box">
            <div>
              <strong>{t('updateFailed')}</strong>
              <details>
                <summary>{t('details')}</summary>
                <pre>{update.error.detail}</pre>
              </details>
            </div>
          </div>
        )}
        <div className="dialog-actions">
          {!busy && <button onClick={update.hide}>{t('later')}</button>}
          <button className="primary" disabled={busy} onClick={() => void update.install()}>
            {busy ? <Spinner /> : <ArrowDownToLine size={16} />}{' '}
            {t(busy ? 'busy' : 'installUpdate')}
          </button>
        </div>
      </section>
    </div>
  );
}
