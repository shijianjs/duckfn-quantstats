import {createElement, useRef} from 'react';
import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import CodeBlock from '@theme/CodeBlock';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import {
  DfkFeatures,
  DfkHero,
  DfkNextSteps,
  registerDfkElements,
  type FeatureItem,
  type HeroAction,
  type HeroBadge,
  type HeroLink,
  type NextStepItem,
} from 'duckfn-docs-kit';

import styles from './index.module.css';

// Defining the `dfk-*` custom elements is a one-time side effect (it also
// registers the official `<iconify-icon>` element). Idempotent, and a no-op
// during Docusaurus' Node prerender pass.
registerDfkElements();

/**
 * The landing page: hero, features, a SQL/report showcase and the "where next"
 * cards.
 *
 * The hero, the feature grid and the next-step cards are `dfk-*` web components
 * from duckfn-docs-kit. A custom element cannot render React's `<Translate>`,
 * so the copy is resolved with the imperative `translate()` API into plain
 * strings for the active locale and handed to the components through their own
 * setters; the strings still live in `i18n/zh-Hans/code.json` under the same
 * `homepage.*` keys. The showcase stays here because it needs the theme's
 * `CodeBlock`.
 *
 * Known trade-off (accepted): the `dfk-*` sections render client-side, so their
 * prerendered HTML is empty until hydration — the same behaviour as the
 * `<iconify-icon>` glyphs.
 */

type DfkTag = 'dfk-hero' | 'dfk-features' | 'dfk-next-steps';

/**
 * Mounts a `dfk-*` element and hands its content over through the component's
 * own setters.
 *
 * React's SSR/hydration path only reconciles string/number props onto custom
 * elements — an object payload is never serialised into the prerendered HTML,
 * so hydration would leave the component empty. A callback ref is the reliable
 * channel: React calls it with the live node after mount, where the setters are
 * invoked.
 */
function dfk<TContent>(
  tag: DfkTag,
  mount: (node: HTMLElement, content: TContent) => void,
  content: TContent,
): ReactNode {
  return createElement(tag, {
    ref: (node: HTMLElement | null) => {
      if (node) {
        mount(node, content);
      }
    },
  });
}

interface HeroContent {
  logoSrc: string;
  title: string;
  tagline: string;
  primary: HeroLink;
  secondary: HeroAction;
  badges: HeroBadge[];
}

function mountHero(node: HTMLElement, content: HeroContent): void {
  const hero = node as DfkHero;
  hero.setLogo(content.logoSrc);
  hero.setTitle(content.title);
  hero.setTagline(content.tagline);
  hero.setPrimaryAction(content.primary);
  hero.setSecondaryAction(content.secondary);
  hero.setBadges(content.badges);
}

interface FeaturesContent {
  sectionTitle: string;
  items: FeatureItem[];
}

function mountFeatures(node: HTMLElement, content: FeaturesContent): void {
  const features = node as DfkFeatures;
  features.setSectionTitle(content.sectionTitle);
  features.setFeatures(content.items);
}

interface NextStepsContent {
  sectionTitle: string;
  items: NextStepItem[];
}

function mountNextSteps(node: HTMLElement, content: NextStepsContent): void {
  const steps = node as DfkNextSteps;
  steps.setSectionTitle(content.sectionTitle);
  steps.setSteps(content.items);
}

/**
 * Kept out of the JSX below on purpose: a template literal written inline would
 * carry the JSX indentation into the rendered code block.
 *
 * The data comes from the demo snapshot this documentation site serves itself
 * (`static/demo/prices.csv`), which is the URL every example in the docs reads.
 */
const SQL_SAMPLE = `-- once per session: INSTALL duckfn_quantstats FROM community; LOAD duckfn_quantstats;
WITH prices AS (
    SELECT *
    FROM read_csv('https://shijianjs.github.io/duckfn-quantstats/demo/prices.csv')
)
SELECT (r).symbol, (r).benchmark, length((r).html) AS html_bytes
FROM (
    SELECT unnest(qs_html_reports_by_prices(
               symbol, date, price,
               {'benchmark': ['SPX'],
                'benchmark_title': ['S&P 500'],
                'title': symbol,
                'strategy_title': symbol}::qs_html_report_options)) AS r
    FROM prices
);`;

/**
 * The shields.io badges ask for `style=flat`, which is the rounded style; the
 * default `flat-square` draws square corners and would clash with the other
 * badges. The row has to look like one set, so the shape is decided at the
 * source rather than patched with CSS.
 *
 * These sit on a page for people who *use* the extension, so the badges answer
 * "where do I get it, what does it cost, where does it run" — not "what is it
 * written in".
 */
function badges(repoUrl: string): HeroBadge[] {
  return [
    {
      href: `${repoUrl}/releases`,
      src: `https://img.shields.io/github/v/release/${repoUrl.replace(/^https:\/\/github\.com\//, '')}?style=flat`,
      alt: 'Latest release',
    },
    {
      href: 'https://duckdb.org/community_extensions/extensions/duckfn_quantstats',
      src: 'https://img.shields.io/badge/DuckDB%20community-extension-14459b.svg?style=flat',
      alt: 'Published in the DuckDB community extensions',
    },
    {
      href: `${repoUrl}/blob/main/LICENSE`,
      src: 'https://img.shields.io/badge/license-MIT-14459b.svg?style=flat',
      alt: 'MIT license',
    },
    {
      href: 'https://duckdb.org',
      src: 'https://img.shields.io/badge/DuckDB-1.5%2B-14459b.svg?style=flat',
      alt: 'DuckDB 1.5 or newer',
    },
  ];
}

function heroContent(
  logoSrc: string,
  introHref: string,
  repoUrl: string,
  title: string,
  tagline: string,
): HeroContent {
  return {
    logoSrc,
    title,
    tagline,
    primary: {
      label: translate({id: 'homepage.getStarted', message: 'Get started'}),
      href: introHref,
    },
    secondary: {
      label: translate({
        id: 'homepage.github',
        description: 'Home page button linking to the repository',
        message: 'GitHub',
      }),
      href: repoUrl,
      icon: 'simple-icons:github',
    },
    badges: badges(repoUrl),
  };
}

function featuresContent(): FeaturesContent {
  return {
    sectionTitle: translate({
      id: 'homepage.features.title',
      description: 'Home page section title above the feature cards',
      message: 'What this extension gives you',
    }),
    items: [
      {
        icon: 'lucide:layout-list',
        title: translate({
          id: 'homepage.features.oneCall.title',
          description: 'Home page feature card title',
          message: 'The whole table, one call',
        }),
        details: translate({
          id: 'homepage.features.oneCall.details',
          description: 'Home page feature card description',
          message:
            'Hand it one date-ordered long table and it groups by symbol inside the aggregate, so the SQL needs no GROUP BY and a hundred instruments still mean one SELECT.',
        }),
      },
      {
        icon: 'lucide:line-chart',
        title: translate({
          id: 'homepage.features.inputs.title',
          description: 'Home page feature card title',
          message: 'Returns or prices',
        }),
        details: translate({
          id: 'homepage.features.inputs.details',
          description: 'Home page feature card description',
          message:
            'qs_html_reports takes periodic returns; qs_html_reports_by_prices takes prices or NAVs and differences them itself, so the pct_change window can stay out of your SQL.',
        }),
      },
      {
        icon: 'lucide:scale',
        title: translate({
          id: 'homepage.features.benchmark.title',
          description: 'Home page feature card title',
          message: 'Benchmarks are symbols',
        }),
        details: translate({
          id: 'homepage.features.benchmark.details',
          description: 'Home page feature card description',
          message:
            'The benchmark is an ordinary symbol of the same table, named in the options; it is input only and gets no report of its own. Several benchmarks mean several reports.',
        }),
      },
      {
        icon: 'lucide:folder-output',
        title: translate({
          id: 'homepage.features.output.title',
          description: 'Home page feature card title',
          message: 'On disk or in the browser',
        }),
        details: translate({
          id: 'homepage.features.output.details',
          description: 'Home page feature card description',
          message:
            "output_dir writes every report through DuckDB's VFS (local disk, s3://, wasm) with a never-colliding name the function generates; open_in_browser opens them once they are there.",
        }),
      },
      {
        icon: 'lucide:globe',
        title: translate({
          id: 'homepage.features.platform.title',
          description: 'Home page feature card title',
          message: 'Any DuckDB client, any platform',
        }),
        details: translate({
          id: 'homepage.features.platform.details',
          description: 'Home page feature card description',
          message:
            "The CLI, Python, Java/JVM, Node, R or DuckDB-Wasm in the browser: one INSTALL brings the same signed extension to every platform DuckDB ships on, and the reports are plain SQL wherever they run. No quantstats install, no Python environment to keep in step.",
        }),
      },
      {
        icon: 'lucide:life-buoy',
        title: translate({
          id: 'homepage.features.docs.title',
          description: 'Home page feature card title',
          message: 'This documentation site',
        }),
        details: translate({
          id: 'homepage.features.docs.details',
          description: 'Home page feature card description',
          message:
            'Bilingual (English and Simplified Chinese), with examples that run in the page against a real snapshot, and the full option reference next to them.',
        }),
      },
    ],
  };
}

function nextStepsContent(
  hrefs: readonly [string, string, string, string],
): NextStepsContent {
  const [quickStart, functions, output, development] = hrefs;
  return {
    sectionTitle: translate({
      id: 'homepage.next.title',
      description: 'Home page section title above the link cards',
      message: 'Where to go next',
    }),
    items: [
      {
        href: quickStart,
        title: translate({
          id: 'homepage.next.quickStart.title',
          description: 'Home page link card title',
          message: 'Quick start',
        }),
        details: translate({
          id: 'homepage.next.quickStart.details',
          description: 'Home page link card description',
          message: 'Install it, point it at your table, and read the first reports.',
        }),
      },
      {
        href: functions,
        title: translate({
          id: 'homepage.next.functions.title',
          description: 'Home page link card title',
          message: 'Functions',
        }),
        details: translate({
          id: 'homepage.next.functions.details',
          description: 'Home page link card description',
          message: 'The two SQL names, the result shape, and how a benchmark becomes reports.',
        }),
      },
      {
        href: output,
        title: translate({
          id: 'homepage.next.output.title',
          description: 'Home page link card title',
          message: 'Reports on disk',
        }),
        details: translate({
          id: 'homepage.next.output.details',
          description: 'Home page link card description',
          message: 'output_dir, the generated file names, and opening a report in the browser.',
        }),
      },
      {
        href: development,
        title: translate({
          id: 'homepage.next.development.title',
          description: 'Home page link card title',
          message: 'Development guide',
        }),
        details: translate({
          id: 'homepage.next.development.details',
          description: 'Home page link card description',
          message: 'For contributors: the internals, the build, the tests, the release.',
        }),
      },
    ],
  };
}

/**
 * The framed report, with its own controls in the top-right corner.
 *
 * Both buttons exist because a report is a tall document squeezed into a
 * 30rem window: **Fullscreen** asks the frame itself for fullscreen (the
 * report keeps its own viewport, so its own layout is what fills the screen),
 * and the second one leaves the page altogether for readers who would rather
 * keep it open in a tab while they read the SQL.
 */
function ReportFrame({src}: {src: string}): ReactNode {
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
        src={src}
        title="quantstats HTML report for GOOGL against the S&P 500"
        loading="lazy"
      />
      <div className={styles.reportActions}>
        <button
          type="button"
          className={styles.reportAction}
          onClick={enterFullscreen}
          title={translate({
            id: 'homepage.showcase.fullscreen',
            description: 'Tooltip of the fullscreen button on the report iframe',
            message: 'Fullscreen',
          })}
          aria-label={translate({
            id: 'homepage.showcase.fullscreen',
            description: 'Tooltip of the fullscreen button on the report iframe',
            message: 'Fullscreen',
          })}>
          {createElement('iconify-icon', {
            icon: 'lucide:maximize-2',
            'aria-hidden': 'true',
          })}
        </button>
        <a
          className={styles.reportAction}
          href={src}
          target="_blank"
          rel="noreferrer"
          title={translate({
            id: 'homepage.showcase.openTab',
            description: 'Tooltip of the open-in-new-tab button on the report iframe',
            message: 'Open in a new tab',
          })}
          aria-label={translate({
            id: 'homepage.showcase.openTab',
            description: 'Tooltip of the open-in-new-tab button on the report iframe',
            message: 'Open in a new tab',
          })}>
          {createElement('iconify-icon', {
            icon: 'lucide:external-link',
            'aria-hidden': 'true',
          })}
        </a>
      </div>
    </div>
  );
}

/**
 * The showcase: one SQL statement on the left, the report it produces on the
 * right.
 *
 * The report is a pre-generated file served from `static/demo/` rather than
 * something the page computes: it is the real output over the demo snapshot,
 * and a static file keeps the landing page free of a DuckDB-Wasm instance.
 */
function SqlAndReport(): ReactNode {
  const reportUrl = useBaseUrl('/demo/qs_report_GOOGL-S&P_500.html');
  return (
    <section className={styles.sectionTint}>
      <div className={styles.sectionInner}>
        <Heading as="h2" className={styles.sectionTitle}>
          <Translate
            id="homepage.showcase.title"
            description="Home page section title above the SQL block and the report">
            One query, a tearsheet per instrument
          </Translate>
        </Heading>
        <p className={styles.sectionLead}>
          <Translate
            id="homepage.showcase.lead"
            description="Home page paragraph introducing the SQL block and the report">
            This is the whole interface: one aggregate call over a long table of
            prices, no grouping to write, no Python. On the right is the report
            it returns for GOOGL, with the S&amp;P 500 as its benchmark — the
            real output, rendered in an iframe.
          </Translate>
        </p>
        <div className={styles.codeGrid}>
          <CodeBlock language="sql" title="duckdb">
            {SQL_SAMPLE}
          </CodeBlock>
          <div className={styles.codeColumn}>
            <ReportFrame src={reportUrl} />
            <p className={styles.codeCaption}>
              <Translate
                id="homepage.showcase.caption"
                description="Home page note under the report iframe">
                The report it produced for GOOGL — charts, metrics table and
                benchmark column included. Scroll inside the frame, or open the
                full report under /demo/.
              </Translate>
            </p>
          </div>
        </div>
        <p className={styles.showcaseLinkRow}>
          <Link className={styles.showcaseLink} to="/docs/guide/functions">
            <Translate
              id="homepage.showcase.link"
              description="Home page link to the functions guide">
              The two functions, in detail
            </Translate>
            {/* The official Iconify web component (registered by
                registerDfkElements()); a string `icon` attribute is all it
                needs. createElement keeps it out of the JSX namespace. */}
            {createElement('iconify-icon', {
              icon: 'lucide:arrow-right',
              className: styles.showcaseLinkArrow,
              'aria-hidden': 'true',
            })}
          </Link>
        </p>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  const repoUrl = siteConfig.customFields?.repoUrl as string;
  // Not a hard-coded "/img/...": the site is published under /<repo>/ on GitHub
  // Pages, and only useBaseUrl adds that prefix. The dfk-* components render
  // plain anchors, so every internal href is resolved here before it is passed
  // in.
  const logoUrl = useBaseUrl('img/logo.svg');
  const introUrl = useBaseUrl('/docs/intro');
  const nextHrefs = [
    useBaseUrl('/docs/getting-started/quick-start'),
    useBaseUrl('/docs/guide/functions'),
    useBaseUrl('/docs/guide/output-and-browser'),
    useBaseUrl('/docs/development-guide/architecture/project-structure'),
  ] as const;

  return (
    <Layout
      title={siteConfig.title}
      description="quantstats HTML tearsheets from SQL: one DuckDB extension, no Python. The functions, the options, and how the reports are written.">
      {/* Layout renders no <main> of its own: this is the page's only one. */}
      <main>
        {dfk(
          'dfk-hero',
          mountHero,
          heroContent(
            logoUrl,
            introUrl,
            repoUrl,
            siteConfig.title,
            translate({id: 'homepage.tagline', message: siteConfig.tagline}),
          ),
        )}
        {dfk('dfk-features', mountFeatures, featuresContent())}
        <SqlAndReport />
        {dfk('dfk-next-steps', mountNextSteps, nextStepsContent(nextHrefs))}
      </main>
    </Layout>
  );
}
