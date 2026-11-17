import { useTranslation } from 'react-i18next';
import { Folder } from 'lucide-react';
import { chooseDirectory } from '../bridge';
import type { DownloadOptions } from '../types';
import { Check } from './Common';
export function Options({
  options,
  onChange,
  destination,
  onDestination,
}: {
  options: DownloadOptions;
  onChange: (p: Partial<DownloadOptions>) => void;
  destination: string;
  onDestination: (p: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="options">
      <label className="field">
        <span>{t('quality')}</span>
        <select
          value={options.maxHeight}
          onChange={(e) => onChange({ maxHeight: Number(e.target.value) })}
          disabled={options.format === 'audio'}
        >
          {[0, 360, 480, 720, 1080, 1440, 2160, 4320].map((h) => (
            <option key={h} value={h}>
              {h ? `${h}p` : t('best')}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>{t('format')}</span>
        <select value={options.format} onChange={(e) => onChange({ format: e.target.value })}>
          {['mp4', 'mkv', 'original', 'audio'].map((f) => (
            <option value={f} key={f}>
              {f === 'mp4' || f === 'mkv' ? f.toUpperCase() : t(f)}
            </option>
          ))}
        </select>
      </label>
      {options.format === 'audio' ? (
        <label className="field">
          <span>{t('audioFormat')}</span>
          <select
            value={options.audioFormat}
            onChange={(e) => onChange({ audioFormat: e.target.value })}
          >
            {['mp3', 'm4a', 'opus'].map((f) => (
              <option key={f}>{f}</option>
            ))}
          </select>
        </label>
      ) : (
        options.format === 'mp4' && (
          <>
            <Check
              label={t('compatibility')}
              checked={options.compatibility}
              onChange={(v) => onChange({ compatibility: v })}
            />
            <p className="hint">{t('compatibilityHint')}</p>
          </>
        )
      )}
      <div className="field">
        <span>{t('destination')}</span>
        <div className="path">
          <Folder size={16} />
          <span title={destination}>{destination || '—'}</span>
        </div>
        <button
          onClick={() => {
            void chooseDirectory().then((p) => {
              if (typeof p === 'string') onDestination(p);
            });
          }}
        >
          {t('chooseFolder')}
        </button>
      </div>
      <div className="checks">
        <Check
          label={t('skipExisting')}
          checked={options.skipExisting}
          onChange={(v) => onChange({ skipExisting: v })}
        />
        <Check
          label={t('subtitles')}
          checked={options.subtitles}
          onChange={(v) => onChange({ subtitles: v })}
        />
        <Check
          label={t('thumbnail')}
          checked={options.thumbnail}
          onChange={(v) => onChange({ thumbnail: v })}
        />
      </div>
    </div>
  );
}
