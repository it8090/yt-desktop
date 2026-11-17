import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Film, AlertCircle, LoaderCircle } from 'lucide-react';
import type { AppError, MediaItem } from '../types';
export function Thumb({ item }: { item: MediaItem }) {
  return (
    <div className="thumb">
      {item.thumbnail ? (
        <img
          src={item.thumbnail}
          alt=""
          loading="lazy"
          referrerPolicy="no-referrer"
          onError={(e) => {
            e.currentTarget.style.display = 'none';
          }}
        />
      ) : (
        <Film size={18} />
      )}
    </div>
  );
}
export function Status({ value }: { value: string }) {
  const { t } = useTranslation();
  return <span className={`status status-${value}`}>{t(value)}</span>;
}
export function Empty({
  icon,
  heading,
  detail,
}: {
  icon: ReactNode;
  heading: string;
  detail?: string;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h2>{heading}</h2>
      {detail && <p>{detail}</p>}
    </div>
  );
}
export function ErrorBox({ error }: { error: AppError }) {
  const { t } = useTranslation();
  return (
    <div className="error-box" role="alert">
      <AlertCircle size={17} />
      <div>
        <strong>{t(`errors.${error.code}`, { defaultValue: t('errorTitle') })}</strong>
        {error.detail && (
          <details>
            <summary>{t('details')}</summary>
            <pre>{error.detail}</pre>
          </details>
        )}
      </div>
    </div>
  );
}
export function Spinner() {
  return <LoaderCircle className="spin" size={16} />;
}
export function Check({
  label,
  checked,
  onChange,
  disabled = false,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className="check">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        disabled={disabled}
      />
      <span>{label}</span>
    </label>
  );
}
