import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ExternalLink, Package, Power, CheckCircle2, AlertCircle } from 'lucide-react';
import { call } from '../bridge';
import { useStore } from '../store';
import type { Settings as SettingsType, ToolInfo } from '../types';
import { Options } from '../components/Options';
import { Check, Spinner } from '../components/Common';
import { useUpdates } from '../updates';
import { applyLanguage } from '../i18n';
export default function Settings() {
  const { t } = useTranslation();
  const update = useUpdates();
  const settings = useStore((s) => s.settings);
  const tools = useStore((s) => s.tools);
  const version = useStore((s) => s.version);
  const [draft, setDraft] = useState<SettingsType>(settings);
  const [saving, setSaving] = useState(false);
  const [installing, setInstalling] = useState(false);
  useEffect(() => setDraft(settings), [settings]);
  const set = (p: Partial<SettingsType>) => setDraft((s) => ({ ...s, ...p }));
  async function save() {
    setSaving(true);
    try {
      await useStore.getState().saveSettings(draft);
      useStore.getState().setNotice(t('saved'));
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setSaving(false);
    }
  }
  async function install(update: boolean) {
    setInstalling(true);
    try {
      const value = await call<ToolInfo[]>('install_tools', { update });
      useStore.getState().event({ type: 'tools', data: value });
      useStore.getState().setNotice(t('toolsReady'));
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setInstalling(false);
    }
  }
  return (
    <section className="page preferences">
      <div className="page-title">
        <div>
          <h1>{t('settingsTitle')}</h1>
          <p>{t('settingsHint')}</p>
        </div>
        <button className="primary" disabled={saving} onClick={() => void save()}>
          {saving && <Spinner />}
          {t('save')}
        </button>
      </div>
      <div className="preferences-grid">
        <div>
          <section className="settings-section">
            <h2>{t('general')}</h2>
            <div className="two-fields">
              <label className="field">
                <span>{t('language')}</span>
                <select
                  value={draft.language}
                  onChange={(e) => {
                    set({ language: e.target.value });
                    applyLanguage(e.target.value);
                  }}
                >
                  <option value="system">{t('system')}</option>
                  <option value="zh-CN">简体中文</option>
                  <option value="en">English</option>
                </select>
              </label>
              <label className="field">
                <span>{t('theme')}</span>
                <select value={draft.theme} onChange={(e) => set({ theme: e.target.value })}>
                  {['system', 'light', 'dark'].map((v) => (
                    <option value={v} key={v}>
                      {t(v)}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <Check
              label={t('closeToTray')}
              checked={draft.closeToTray}
              onChange={(v) => set({ closeToTray: v })}
            />
          </section>
          <section className="settings-section">
            <h2>{t('downloadDefaults')}</h2>
            <Options
              options={draft.options}
              onChange={(p) => set({ options: { ...draft.options, ...p } })}
              destination={draft.destination}
              onDestination={(destination) => set({ destination })}
            />
            <div className="two-fields">
              <label className="field">
                <span>{t('concurrency')}</span>
                <select
                  value={draft.concurrency}
                  onChange={(e) => set({ concurrency: Number(e.target.value) })}
                >
                  {[1, 2, 3, 4].map((n) => (
                    <option key={n}>{n}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>{t('retries')}</span>
                <input
                  type="number"
                  min={0}
                  max={10}
                  value={draft.retries}
                  onChange={(e) => set({ retries: Number(e.target.value) })}
                />
              </label>
            </div>
          </section>
          <section className="settings-section">
            <h2>{t('network')}</h2>
            <label className="field">
              <span>{t('proxy')}</span>
              <select value={draft.proxyMode} onChange={(e) => set({ proxyMode: e.target.value })}>
                <option value="system">{t('system')}</option>
                <option value="none">{t('proxyNone')}</option>
                <option value="custom">{t('proxyCustom')}</option>
              </select>
            </label>
            {draft.proxyMode === 'custom' && (
              <label className="field">
                <span>{t('proxyUrl')}</span>
                <input
                  value={draft.proxyUrl}
                  onChange={(e) => set({ proxyUrl: e.target.value })}
                  placeholder="http://127.0.0.1:7890"
                />
              </label>
            )}
            <p className="hint">{t('proxyHint')}</p>
          </section>
        </div>
        <div>
          <section className="settings-section components">
            <h2>
              <Package size={17} />
              {t('components')}
            </h2>
            <p className="hint">{t('componentHint')}</p>
            {['yt-dlp', 'ffmpeg', 'ffprobe', 'deno'].map((name) => {
              const tool = tools.find((v) => v.name === name);
              return (
                <div className="tool" key={name}>
                  <div>
                    {tool?.version ? <CheckCircle2 size={16} /> : <AlertCircle size={16} />}
                    <strong>{name}</strong>
                    <small>
                      {t(tool?.path ? (tool.managed ? 'managed' : 'systemTool') : 'missing')}
                    </small>
                  </div>
                  {tool?.version && <p title={tool.path || ''}>{tool.version}</p>}
                </div>
              );
            })}
            <div className="vertical-buttons">
              <button disabled={installing} onClick={() => void install(false)}>
                {installing && <Spinner />}
                {t(installing ? 'installing' : 'install')}
              </button>
              <button disabled={installing} onClick={() => void install(true)}>
                {t('update')}
              </button>
            </div>
          </section>
          <section className="settings-section">
            <h2>{t('updates')}</h2>
            <Check
              label={t('autoUpdate')}
              checked={draft.autoUpdate}
              onChange={(autoUpdate) => set({ autoUpdate })}
            />
            <p className="hint">{t('updateHint')}</p>
            <div className="app-update-check">
              <button
                disabled={['checking', 'downloading', 'installing'].includes(update.status)}
                onClick={() => void update.check()}
              >
                {update.status === 'checking' && <Spinner />}
                {t(update.status === 'checking' ? 'checkingUpdate' : 'checkUpdate')}
              </button>
              {update.info && (
                <button className="primary" onClick={update.show}>
                  {t('updateAvailable', { version: update.info.version })}
                </button>
              )}
            </div>
            {update.status === 'current' && <p className="update-state">{t('upToDate')}</p>}
            {update.error && (
              <details className="update-state error">
                <summary>{t('updateFailed')}</summary>
                <pre>{update.error.detail}</pre>
              </details>
            )}
          </section>
          <section className="settings-section">
            <h2>{t('about')}</h2>
            <p>
              YT-Desktop <span className="muted">{version}</span>
            </p>
            <button
              className="text-button"
              onClick={() =>
                void call('open_url', { url: 'https://github.com/it8090/yt-desktop' }).catch(
                  useStore.getState().setError,
                )
              }
            >
              <ExternalLink size={14} />
              {t('github')}
            </button>
            <button
              className="quit"
              onClick={() => void call('quit').catch(useStore.getState().setError)}
            >
              <Power size={14} />
              {t('quit')}
            </button>
          </section>
        </div>
      </div>
    </section>
  );
}
