import {createElement} from 'react';
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
 * The landing page: hero, features, a Rust/SQL showcase and the "where next"
 * cards.
 *
 * The hero, the feature grid and the next-step cards are `dfk-*` web components
 * from duckfn-docs-kit. A custom element cannot render React's `<Translate>`,
 * so the copy is resolved with the imperative `translate()` API into plain
 * strings for the active locale and handed to the components through their own
 * setters; the strings still live in `i18n/zh-Hans/code.json` under the same
 * `homepage.*` keys. The code showcase stays here because it needs the theme's
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
 * carry the JSX indentation into the rendered code block. This is the real
 * `qs_html_reports` (src/extension/functions/aggregate_html/html_returns.rs),
 * trimmed to the parts worth showing.
 */
const RUST_SAMPLE = `use duckfn::{DuckAggregateState, DuckDate, DuckLazy, DuckResult, duck_aggregate_function};

/// A DuckDB aggregate: one attribute, one ordinary Rust function.
#[duck_aggregate_function(
    description = "Renders one quantstats HTML report per symbol from a long table of periodic returns",
    comment = "Groups by symbol internally, so the SQL needs no GROUP BY",
)]
pub(super) fn qs_html_reports(
    symbol: String,
    date: DuckDate,
    period_return: f64,
    options: Option<DuckLazy<QuantstatsHtmlOptions>>,
    state: &mut HtmlReportsState,      // the aggregate's state
) -> DuckResult<()> {
    state.symbols.push(
        &symbol,
        options.as_ref(),
        SeriesPoint {days_since_epoch: date.days_since_epoch, value: period_return},
    )
}`;

/** The SQL half of the showcase: the whole interface, one call per instrument. */
const SQL_SAMPLE = `-- a locally built extension loads with -unsigned
LOAD './target/debug/duckfn_quantstats.duckdb_extension';

-- one call, one report per symbol, no GROUP BY
SELECT (r).symbol, length((r).html) AS html_bytes
FROM (SELECT unnest(qs_html_reports_by_prices(symbol, date, price, NULL)) AS r
      FROM prices);
-- MSFT | 421337
-- GOOGL | 420119`;

/**
 * The shields.io badges ask for `style=flat`, which is the rounded style; the
 * default `flat-square` draws square corners and would clash with the language
 * badges below, which are rounded too. The row has to look like one set, so the
 * shape is decided at the source rather than patched with CSS.
 */
function badges(repoUrl: string): HeroBadge[] {
  return [
    {
      href: `${repoUrl}/releases`,
      src: `https://img.shields.io/github/v/release/${repoUrl.replace(/^https:\/\/github\.com\//, '')}?style=flat`,
      alt: 'Latest release',
    },
    {
      href: `${repoUrl}/blob/main/LICENSE`,
      src: 'https://img.shields.io/badge/license-MIT-14459b.svg?style=flat',
      alt: 'MIT license',
    },
    {
      href: 'https://rust-lang.org',
      src: 'https://img.shields.io/badge/Rust-1.86%2B-14459b.svg?style=flat',
      alt: 'Rust 1.86 or newer',
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
            "The benchmark is an ordinary symbol of the same table, named in the options; it is input only and gets no report of its own. Several benchmarks mean several reports.",
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
            "output_dir writes every report through DuckDB's VFS (local, s3://, wasm) with a never-colliding name the function generates; open_in_browser opens them once they are there.",
        }),
      },
      {
        icon: 'lucide:package',
        title: translate({
          id: 'homepage.features.build.title',
          description: 'Home page feature card title',
          message: 'No local DuckDB build',
        }),
        details: translate({
          id: 'homepage.features.build.details',
          description: 'Home page feature card description',
          message:
            'Headers only, dispatched through DuckDB\u2019s API table at load time, so one cargo command produces the .duckdb_extension \u2014 no CMake, no C++ toolchain.',
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
            'Docusaurus in docs/, bilingual (English and Simplified Chinese), with runnable SQL blocks powered by duckfn-docs-kit and a workflow that publishes it to GitHub Pages on every version tag.',
        }),
      },
    ],
  };
}

function nextStepsContent(
  hrefs: readonly [string, string, string, string],
): NextStepsContent {
  const [quickStart, structure, functions, release] = hrefs;
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
          message: 'Install the extension or build it, then produce the first reports from SQL.',
        }),
      },
      {
        href: structure,
        title: translate({
          id: 'homepage.next.structure.title',
          description: 'Home page link card title',
          message: 'Project structure',
        }),
        details: translate({
          id: 'homepage.next.structure.details',
          description: 'Home page link card description',
          message: 'Where the entry point, the functions and the SQL types live.',
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
          message: 'The two SQL names, the options, and how a benchmark turns into reports.',
        }),
      },
      {
        href: release,
        title: translate({
          id: 'homepage.next.release.title',
          description: 'Home page link card title',
          message: 'Build and release',
        }),
        details: translate({
          id: 'homepage.next.release.details',
          description: 'Home page link card description',
          message: 'The two build paths, the release flow and the wasm target.',
        }),
      },
    ],
  };
}

function CodeShowcase(): ReactNode {
  return (
    <section className={styles.sectionTint}>
      <div className={styles.sectionInner}>
        <Heading as="h2" className={styles.sectionTitle}>
          <Translate
            id="homepage.showcase.title"
            description="Home page section title above the Rust and SQL code blocks">
            One attribute = one SQL function
          </Translate>
        </Heading>
        <p className={styles.sectionLead}>
          <Translate
            id="homepage.showcase.lead"
            description="Home page paragraph introducing the Rust and SQL code blocks">
            The attribute generates the FFI wrapper, the argument readers and
            the registration code. The body on the left is the real aggregate —
            it decides how a row joins the group, and the tail renders one
            report per symbol.
          </Translate>
        </p>
        <div className={styles.codeGrid}>
          <CodeBlock
            language="rust"
            title="src/extension/functions/aggregate_html/html_returns.rs">
            {RUST_SAMPLE}
          </CodeBlock>
          <div className={styles.codeColumn}>
            <CodeBlock language="sql" title="duckdb -unsigned">
              {SQL_SAMPLE}
            </CodeBlock>
            {/* Balances the two columns, and explains the trailing comments. */}
            <p className={styles.codeCaption}>
              <Translate
                id="homepage.showcase.caption"
                description="Home page note under the SQL code block explaining the trailing comments">
                The comments are what each call returns. Loading needs -unsigned,
                because a locally built extension is not signed by DuckDB's
                distribution key.
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
    useBaseUrl('/docs/getting-started/project-structure'),
    useBaseUrl('/docs/guide/functions'),
    useBaseUrl('/docs/build-and-release'),
  ] as const;

  return (
    <Layout
      title={siteConfig.title}
      description="Documentation for duckfn_quantstats: the SQL functions it registers, the options, and how it is built and released.">
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
        <CodeShowcase />
        {dfk('dfk-next-steps', mountNextSteps, nextStepsContent(nextHrefs))}
      </main>
    </Layout>
  );
}
