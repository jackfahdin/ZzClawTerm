import React from 'react';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import Translate, {translate} from '@docusaurus/Translate';

import styles from './changelog.module.css';
import {getChangelogReleases} from '../data/changelogData';

function formatVersionLabel(version: string) {
  const match = version.match(/^\[(.+?)\]\s*-\s*(.+)$/);
  if (!match) {
    return {tag: version, date: ''};
  }

  const isSemver = /^\d+\.\d+\.\d+/.test(match[1]);

  return {
    tag: isSemver ? `v${match[1]}` : match[1],
    date: match[2],
  };
}

function renderItem(item: string) {
  const boldPrefixMatch = item.match(/^\*\*(.+?):\*\*\s*(.+)$/);

  if (!boldPrefixMatch) {
    return item;
  }

  return (
    <>
      <strong>{boldPrefixMatch[1]}:</strong> {boldPrefixMatch[2]}
    </>
  );
}

export default function ChangelogPage(): React.ReactElement {
  const {
    i18n: {currentLocale},
  } = useDocusaurusContext();
  const changelogReleases = getChangelogReleases(currentLocale);

  return (
    <Layout
      title={translate({message: '更新日志'})}
      description={translate({message: '查看 NyaTerm 的版本演进与功能更新记录'})}>
      <main className={styles.page}>
        <section className={styles.hero}>
          <div className="container">
            <div className={styles.heroInner}>
              <span className={styles.eyebrow}>
                <Translate>Changelog</Translate>
              </span>
              <Heading as="h1" className={styles.title}>
                <Translate>项目日志与版本演进</Translate>
              </Heading>
              <p className={styles.description}>
                {currentLocale === 'zh-CN'
                  ? '早期版本记录包含 GPUI 迁移前的历史实现（包括 Tauri/WebView）；当前应用架构以 GPUI 原生实现为准。'
                  : 'Earlier entries include pre-GPUI history such as Tauri/WebView implementation details; the current application architecture is native GPUI.'}
              </p>
            </div>
          </div>
        </section>

        <section className={styles.timelineSection}>
          <div className="container">
            <div className={styles.timeline}>
              {changelogReleases.map((release) => {
                const {tag, date} = formatVersionLabel(release.version);

                return (
                  <article key={release.version} className={styles.releaseCard}>
                    <div className={styles.releaseHeader}>
                      <div>
                        <Heading as="h2" className={styles.releaseTag}>
                          {tag}
                        </Heading>
                        {date ? <p className={styles.releaseDate}>{date}</p> : null}
                      </div>
                    </div>

                    <div className={styles.sectionList}>
                      {release.sections.map((section) => (
                        <section key={`${release.version}-${section.title}`} className={styles.sectionCard}>
                          <Heading as="h3" className={styles.sectionTitle}>
                            {section.title}
                          </Heading>
                          <ul className={styles.itemList}>
                            {section.items.map((item) => (
                              <li key={item} className={styles.item}>
                                {renderItem(item)}
                              </li>
                            ))}
                          </ul>
                        </section>
                      ))}
                    </div>
                  </article>
                );
              })}
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}
