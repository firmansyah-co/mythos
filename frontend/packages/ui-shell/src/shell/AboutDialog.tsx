import { useEffect, useRef } from 'react';
import { APP_CHANNEL, APP_VERSION } from '../lib/appVersion';

export function AboutDialog({ onClose }: { onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open && typeof element.showModal === 'function') element.showModal();
  }, []);

  return (
    <dialog ref={dialog} className="dialog" onClose={onClose} aria-label="About MythOS Workbench">
      <h2>MythOS Workbench</h2>
      <dl className="props">
        <dt>Version</dt>
        <dd className="mono" data-testid="about-version">
          {APP_VERSION}
        </dd>
        <dt>Build channel</dt>
        <dd className="mono" data-testid="about-channel">
          {APP_CHANNEL}
        </dd>
      </dl>
      <div className="dialog__actions">
        <button type="button" className="btn btn--primary" onClick={onClose}>
          Close
        </button>
      </div>
    </dialog>
  );
}
