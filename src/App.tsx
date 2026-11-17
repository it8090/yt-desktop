import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Plus,
  Download,
  CheckCircle2,
  ShieldCheck,
  Settings as SettingsIcon,
  ListVideo,
  X,
  ArrowRight,
} from 'lucide-react';
import { native } from './bridge';
import { activeStatuses, initialize, useStore } from './store';
import type { Page } from './types';
import { ErrorBox } from './components/Common';
import { UpdateDialog } from './components/UpdateDialog';
import { startupUpdateCheck } from './updates';
import NewDownload from './pages/NewDownload';
import Downloads from './pages/Downloads';
import Auth from './pages/Auth';
import Settings from './pages/Settings';
export default function App() {
  const { t, i18n } = useTranslation();
  const initialized = useStore((s) => s.initialized);
  useEffect(() => {
    if (initialized) startupUpdateCheck(useStore.getState().settings.autoUpdate);
  }, [initialized]);
  const { page, settings, auth, sources, sourceId, jobs, error, notice } = useStore();
  useEffect(() => {
    if (native) void initialize();
  }, []);
  useEffect(() => {
    document.documentElement.lang = i18n.language;
  }, [i18n.language]);
  useEffect(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      document.documentElement.dataset.theme =
        settings.theme === 'system' ? (media.matches ? 'dark' : 'light') : settings.theme;
    };
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [settings.theme]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => useStore.getState().setNotice(null), 6500);
    return () => clearTimeout(timer);
  }, [notice]);
  const nav = [
    ['new', Plus, 0],
    [
      'queue',
      Download,
      jobs.filter((j) => activeStatuses.has(j.status) || j.status === 'paused').length,
    ],
    ['completed', CheckCircle2, jobs.filter((j) => j.status === 'completed').length],
  ] as const;
  const navigate = (p: Page) => useStore.getState().setPage(p);
  return (
    <div className="app">
      <UpdateDialog />
      <aside className="sidebar">
        <div className="brand">
          <img src="/icon.svg" alt="" />
          <strong>YT-Desktop</strong>
        </div>
        <nav>
          {nav.map(([id, Icon, count]) => (
            <button
              key={id}
              className={page === id ? 'nav-item current' : 'nav-item'}
              onClick={() => navigate(id)}
            >
              <Icon size={18} />
              <span>{t(id)}</span>
              {count > 0 && <small>{count}</small>}
            </button>
          ))}
        </nav>
        <div className="recent">
          <h3>{t('recent')}</h3>
          {sources.slice(0, 7).map((s) => (
            <button
              title={s.title || s.urls[0]}
              key={s.id}
              className={
                s.id === sourceId && page === 'new' ? 'recent-item current' : 'recent-item'
              }
              onClick={() => useStore.getState().setSource(s.id)}
            >
              <ListVideo size={15} />
              <span>{s.title || s.urls[0]}</span>
            </button>
          ))}
        </div>
        <div className="sidebar-bottom">
          <button
            className={`nav-item ${page === 'auth' ? 'current' : ''}`}
            onClick={() => navigate('auth')}
          >
            <ShieldCheck size={18} />
            <span>{t('auth')}</span>
            {auth.status === 'expired' && <i className="warning-dot" />}
          </button>
          <button
            className={`nav-item ${page === 'settings' ? 'current' : ''}`}
            onClick={() => navigate('settings')}
          >
            <SettingsIcon size={18} />
            <span>{t('settings')}</span>
          </button>
          <div className="sidebar-foot">YT-Desktop 0.1.0 · {t('local')}</div>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <span>{t(page)}</span>
          <button
            className={`connection-link ${auth.status === 'expired' ? 'warning' : ''}`}
            onClick={() => navigate('auth')}
          >
            <ShieldCheck size={15} />
            YouTube · {t(auth.status)}
          </button>
        </header>
        {!native && (
          <div className="preview-banner">
            {t('nativeOnly')}
            <button
              onClick={() => {
                void i18n.changeLanguage(i18n.language === 'en' ? 'zh-CN' : 'en');
              }}
            >
              {i18n.language === 'en' ? '简体中文' : 'English'}
            </button>
          </div>
        )}
        <main>
          {page === 'new' ? (
            <NewDownload />
          ) : page === 'queue' ? (
            <Downloads key="queue" />
          ) : page === 'completed' ? (
            <Downloads key="completed" completed />
          ) : page === 'auth' ? (
            <Auth />
          ) : (
            <Settings />
          )}
        </main>
      </div>
      {error && (
        <div className="toast error-toast">
          <ErrorBox error={error} />
          <button
            className="icon-button"
            aria-label={t('dismiss')}
            onClick={() => useStore.getState().setError(null)}
          >
            <X size={17} />
          </button>
        </div>
      )}
      {notice && (
        <div className="toast notice-toast" role="status">
          <CheckCircle2 size={18} />
          <span>{notice}</span>
          <button className="text-button" onClick={() => navigate('queue')}>
            {t('viewQueue')}
            <ArrowRight size={14} />
          </button>
          <button
            className="icon-button"
            aria-label={t('dismiss')}
            onClick={() => useStore.getState().setNotice(null)}
          >
            <X size={15} />
          </button>
        </div>
      )}
    </div>
  );
}
