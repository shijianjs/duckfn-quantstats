import {createElement, useRef} from 'react';
import type {ReactNode} from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';

import styles from '@site/src/pages/index.module.css';

export interface ReportFrameProps {
  /** Site-root-relative report URL; resolved against the page's baseUrl. */
  src: string;
  /** Tooltip and accessible name of the fullscreen button. */
  fullscreen: string;
  /** Tooltip and accessible name of the open-in-new-tab button. */
  openTab: string;
}

/**
 * The framed quantstats report, with its own controls in the top-right corner.
 *
 * Both buttons exist because a report is a tall document squeezed into a 30rem
 * window: **Fullscreen** asks the frame itself for fullscreen (the report keeps
 * its own viewport, so its own layout is what fills the screen), and the second
 * one leaves the page altogether for readers who would rather keep it open in a
 * tab while they read the SQL.
 *
 * A component rather than markup in the MDX page: the fullscreen button needs
 * the frame's node, and the URL has to go through `useBaseUrl`. The two labels
 * are props so each locale's page carries its own copy — the landing page no
 * longer routes text through `code.json`.
 */
export default function ReportFrame({
  src,
  fullscreen,
  openTab,
}: ReportFrameProps): ReactNode {
  const url = useBaseUrl(src);
  const frameRef = useRef<HTMLIFrameElement>(null);

  const enterFullscreen = (): void => {
    const frame = frameRef.current;
    if (frame?.requestFullscreen) {
      void frame.requestFullscreen();
    } else {
      // Older engines only have the prefixed form; typed as `any` because the
      // DOM types no longer declare it.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      void (frame as any)?.webkitRequestFullscreen?.();
    }
  };

  return (
    <div className={styles.reportWrap}>
      <iframe
        ref={frameRef}
        className={styles.reportFrame}
        src={url}
        title="quantstats HTML report"
        loading="lazy"
      />
      <div className={styles.reportActions}>
        <button
          type="button"
          className={styles.reportAction}
          onClick={enterFullscreen}
          title={fullscreen}
          aria-label={fullscreen}>
          {createElement('iconify-icon', {
            icon: 'lucide:maximize-2',
            'aria-hidden': 'true',
          })}
        </button>
        <a
          className={styles.reportAction}
          href={url}
          target="_blank"
          rel="noreferrer"
          title={openTab}
          aria-label={openTab}>
          {createElement('iconify-icon', {
            icon: 'lucide:external-link',
            'aria-hidden': 'true',
          })}
        </a>
      </div>
    </div>
  );
}
