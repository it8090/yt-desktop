import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ShieldCheck, ExternalLink, FileKey, Globe } from 'lucide-react';
import { call, chooseCookie } from '../bridge';
import { useStore } from '../store';
import type { AuthConnection } from '../types';
import { timestamp } from '../format';
import { Check, ErrorBox, Spinner, Status } from '../components/Common';
export default function Auth() {
  const { t } = useTranslation();
  const auth = useStore((s) => s.auth);
  const [method, setMethod] = useState(auth.method === 'cookies' ? 'cookies' : 'browser');
  const [browser, setBrowser] = useState(auth.browser || 'chrome');
  const [profile, setProfile] = useState(auth.profile);
  const [cookie, setCookie] = useState<string | null>(null);
  const [consent, setConsent] = useState(false);
  const [remember, setRemember] = useState(auth.remember);
  const [busy, setBusy] = useState(false);
  async function connect() {
    setBusy(true);
    try {
      const result = await call<AuthConnection>('connect_auth', {
        request: {
          method,
          browser,
          profile,
          cookiePath: cookie,
          remember: method === 'cookies' && remember,
          consent,
          testUrl: null,
        },
      });
      useStore.getState().event({ type: 'auth', data: result });
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setBusy(false);
    }
  }
  async function disconnect() {
    try {
      const result = await call<AuthConnection>('disconnect_auth');
      useStore.getState().event({ type: 'auth', data: result });
    } catch (e) {
      useStore.getState().setError(e);
    }
  }
  return (
    <section className="page settings-page">
      <div className="page-title">
        <div>
          <h1>{t('authTitle')}</h1>
          <p>{t('authHint')}</p>
        </div>
      </div>
      <div className="auth-grid">
        <div>
          <div className="connection-card">
            <ShieldCheck size={24} />
            <div>
              <strong>YouTube</strong>
              <p>{t('authResume')}</p>
            </div>
            <Status value={auth.status} />
          </div>
          <p className="explanation">{t('authExplanation')}</p>
          <div className="segmented">
            <button
              className={method === 'browser' ? 'selected' : ''}
              onClick={() => {
                setMethod('browser');
                setConsent(false);
              }}
              disabled={busy}
            >
              <Globe size={16} />
              {t('browserMethod')}
            </button>
            <button
              className={method === 'cookies' ? 'selected' : ''}
              onClick={() => {
                setMethod('cookies');
                setConsent(false);
              }}
              disabled={busy}
            >
              <FileKey size={16} />
              {t('cookieMethod')}
            </button>
          </div>
          {method === 'browser' ? (
            <>
              <label className="field">
                <span>{t('browser')}</span>
                <select value={browser} onChange={(e) => setBrowser(e.target.value)}>
                  {[
                    'chrome',
                    'chromium',
                    'firefox',
                    'edge',
                    'brave',
                    'vivaldi',
                    'opera',
                    'safari',
                  ].map((b) => (
                    <option key={b}>{b}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>{t('profile')}</span>
                <input
                  value={profile}
                  onChange={(e) => setProfile(e.target.value)}
                  placeholder={t('profilePlaceholder')}
                />
              </label>
              <p className="hint">{t('profileHint')}</p>
              <button
                onClick={() =>
                  void call('open_url', { url: 'https://www.youtube.com/' }).catch(
                    useStore.getState().setError,
                  )
                }
              >
                <ExternalLink size={15} />
                {t('loginBrowser')}
              </button>
            </>
          ) : (
            <>
              <div className="field">
                <span>{t('cookieFile')}</span>
                <div className="file-select">
                  <span title={cookie || ''}>{cookie || t('noFile')}</span>
                  <button
                    onClick={() =>
                      void chooseCookie()
                        .then((p) => {
                          if (typeof p === 'string') setCookie(p);
                        })
                        .catch(useStore.getState().setError)
                    }
                  >
                    {t('selectFile')}
                  </button>
                </div>
              </div>
              <Check label={t('remember')} checked={remember} onChange={setRemember} />
              <p className="hint">{t('rememberHint')}</p>
            </>
          )}
          <div className="auth-consent">
            <Check
              label={t(method === 'browser' ? 'consent' : 'cookieConsent')}
              checked={consent}
              onChange={setConsent}
            />
          </div>
          {auth.error && <ErrorBox error={auth.error} />}
          <div className="button-group">
            <button
              className="primary"
              disabled={busy || !consent || (method === 'cookies' && !cookie)}
              onClick={() => void connect()}
            >
              {busy ? <Spinner /> : <ShieldCheck size={16} />} {t(busy ? 'connecting' : 'connect')}
            </button>
            {auth.method !== 'none' && (
              <button disabled={busy} onClick={() => void disconnect()}>
                {t('disconnect')}
              </button>
            )}
          </div>
          {auth.verifiedAt && (
            <p className="hint">{t('verifiedAt', { time: timestamp(auth.verifiedAt) })}</p>
          )}
        </div>
        <aside className="privacy-panel">
          <ShieldCheck size={24} />
          <h2>{t('auth')}</h2>
          <p>{t('authPrivacy')}</p>
        </aside>
      </div>
    </section>
  );
}
