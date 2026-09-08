import React, {useCallback, useEffect, useMemo, useState} from 'react';
import clsx from 'clsx';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';

import styles from './index.module.css';

type FeatureTab = {
  value: string;
  label: string;
  title: string;
  description: string;
  bullets: string[];
  lightImage: string;
  darkImage: string;
};

type FeatureTabWithUrls = FeatureTab & {
  lightImageUrl: string;
  darkImageUrl: string;
};

type DownloadPlatformKey =
  | 'windows-x86_64'
  | 'windows-aarch64'
  | 'windows-x86_64-portable'
  | 'windows-aarch64-portable'
  | 'linux-x86_64'
  | 'linux-aarch64'
  | 'darwin-x86_64'
  | 'darwin-aarch64';

type DownloadPlatform = {
  key: DownloadPlatformKey;
  href: string;
};

type LatestDownloadManifest = {
  version?: string;
  platforms?: Partial<Record<DownloadPlatformKey, {url?: string}>>;
};

// downloads.json is intentionally separate from latest.json. The latter remains a
// signed Tauri updater bridge for installed pre-GPUI releases.
const latestDownloadManifestUrl = 'https://downloads.nyaterm.app/downloads.json';
const downloadBaseUrl = 'https://downloads.nyaterm.app';

// Fallback for the initial render and older manifests that predate portable targets.
// The application updater itself never uses this unsigned URL derivation.
const fallbackPortableVersion = 'latest';

const portableArchByKey: Partial<Record<DownloadPlatformKey, string>> = {
  'windows-x86_64-portable': 'x64',
  'windows-aarch64-portable': 'arm64',
};

function buildPortableHref(key: DownloadPlatformKey, version: string): string | undefined {
  const arch = portableArchByKey[key];
  if (!arch) {
    return undefined;
  }

  return `${downloadBaseUrl}/releases/v${version}/NyaTerm_${version}_windows_${arch}_portable.zip`;
}
const downloadPlatforms: DownloadPlatform[] = [
  {
    key: 'windows-x86_64',
    href: 'https://nyaterm.app/download/windows-x86_64',
  },
  {
    key: 'windows-aarch64',
    href: 'https://nyaterm.app/download/windows-aarch64',
  },
  {
    key: 'windows-x86_64-portable',
    href: buildPortableHref('windows-x86_64-portable', fallbackPortableVersion) ?? '',
  },
  {
    key: 'windows-aarch64-portable',
    href: buildPortableHref('windows-aarch64-portable', fallbackPortableVersion) ?? '',
  },
  {
    key: 'linux-x86_64',
    href: 'https://nyaterm.app/download/linux-x86_64',
  },
  {
    key: 'linux-aarch64',
    href: 'https://nyaterm.app/download/linux-aarch64',
  },
  {
    key: 'darwin-x86_64',
    href: 'https://nyaterm.app/download/darwin-x86_64',
  },
  {
    key: 'darwin-aarch64',
    href: 'https://nyaterm.app/download/darwin-aarch64',
  },
];

const featureAutoplayStoppedKey = 'nyaterm-home-feature-tabs-autoplay-stopped';
const featureAutoplayIntervalMs = 4200;
const featurePreviewImageCache = new Map<string, 'loaded' | Promise<boolean>>();

type ThemeMode = 'light' | 'dark';

function resolveAssetUrl(baseUrl: string, assetPath: string) {
  if (/^(?:[a-z]+:)?\/\//i.test(assetPath)) {
    return assetPath;
  }

  const normalizedBase = baseUrl.endsWith('/') ? baseUrl : `${baseUrl}/`;
  const normalizedPath = assetPath.startsWith('/') ? assetPath.slice(1) : assetPath;
  return `${normalizedBase}${normalizedPath}`;
}

function isFeaturePreviewImageLoaded(url: string) {
  return featurePreviewImageCache.get(url) === 'loaded';
}

function preloadFeaturePreviewImage(url: string): Promise<boolean> {
  if (!url) {
    return Promise.resolve(false);
  }

  const cached = featurePreviewImageCache.get(url);
  if (cached === 'loaded') {
    return Promise.resolve(true);
  }

  if (cached) {
    return cached;
  }

  if (typeof Image === 'undefined') {
    return Promise.resolve(false);
  }

  const pending = new Promise<boolean>((resolve) => {
    const image = new Image();

    image.onload = () => {
      featurePreviewImageCache.set(url, 'loaded');
      resolve(true);
    };

    image.onerror = () => {
      featurePreviewImageCache.delete(url);
      resolve(false);
    };

    image.src = url;
  });

  featurePreviewImageCache.set(url, pending);
  return pending;
}

function getThemeModeFromDom(): ThemeMode {
  if (typeof document === 'undefined') {
    return 'light';
  }

  return document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
}

function useThemeMode() {
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => getThemeModeFromDom());

  useEffect(() => {
    if (typeof document === 'undefined') {
      return;
    }

    const root = document.documentElement;
    const syncThemeMode = () => setThemeMode(getThemeModeFromDom());
    syncThemeMode();

    const observer = new MutationObserver(syncThemeMode);
    observer.observe(root, {attributes: true, attributeFilter: ['data-theme']});

    return () => observer.disconnect();
  }, []);

  return themeMode;
}

function detectDownloadPlatform(): DownloadPlatform {
  if (typeof navigator === 'undefined') {
    return downloadPlatforms[0];
  }

  const platform = navigator.platform.toLowerCase();
  const userAgent = navigator.userAgent.toLowerCase();

  if (platform.includes('mac') || userAgent.includes('mac os')) {
    return downloadPlatforms.find((item) => item.key === 'darwin-x86_64') ?? downloadPlatforms[0];
  }

  if (platform.includes('linux') || userAgent.includes('linux')) {
    return downloadPlatforms.find((item) => item.key === 'linux-x86_64') ?? downloadPlatforms[0];
  }

  return downloadPlatforms[0];
}

function getDownloadPlatformByKey(key: DownloadPlatformKey, platforms: DownloadPlatform[] = downloadPlatforms) {
  return platforms.find((item) => item.key === key) ?? platforms[0];
}

function isArmArchitecture(architecture: string) {
  return architecture.includes('arm') || architecture.includes('aarch64');
}

function getPlatformKeyFromHints(os: string, architecture: string): DownloadPlatformKey {
  const normalizedOs = os.toLowerCase();
  const normalizedArchitecture = architecture.toLowerCase();
  const isArm = isArmArchitecture(normalizedArchitecture);

  if (normalizedOs.includes('mac')) {
    return isArm ? 'darwin-aarch64' : 'darwin-x86_64';
  }

  if (normalizedOs.includes('linux')) {
    return isArm ? 'linux-aarch64' : 'linux-x86_64';
  }

  if (normalizedOs.includes('win')) {
    return isArm ? 'windows-aarch64' : 'windows-x86_64';
  }

  return 'windows-x86_64';
}

function getDownloadPlatformsFromManifest(manifest: LatestDownloadManifest): DownloadPlatform[] {
  return downloadPlatforms.map((platform) => {
    const manifestHref = manifest.platforms?.[platform.key]?.url;
    if (manifestHref) {
      return {...platform, href: manifestHref};
    }

    if (portableArchByKey[platform.key] && manifest.version) {
      const fallbackHref = buildPortableHref(platform.key, manifest.version);
      return fallbackHref ? {...platform, href: fallbackHref} : platform;
    }

    return platform;
  });
}

function getDownloadPlatformLabel(key: DownloadPlatformKey) {
  switch (key) {
    case 'windows-x86_64':
      return translate({message: 'Windows x86_64'});
    case 'windows-aarch64':
      return translate({message: 'Windows ARM64'});
    case 'windows-x86_64-portable':
      return translate({message: 'Windows x86_64 便携版'});
    case 'windows-aarch64-portable':
      return translate({message: 'Windows ARM64 便携版'});
    case 'linux-x86_64':
      return translate({message: 'Linux x86_64'});
    case 'linux-aarch64':
      return translate({message: 'Linux ARM64'});
    case 'darwin-x86_64':
      return translate({message: 'macOS Intel'});
    case 'darwin-aarch64':
      return translate({message: 'macOS Apple Silicon'});
    default:
      return key;
  }
}

function DownloadButton() {
  const [platforms, setPlatforms] = useState<DownloadPlatform[]>(downloadPlatforms);
  const [platformKey, setPlatformKey] = useState<DownloadPlatformKey>(downloadPlatforms[0].key);
  const platform = getDownloadPlatformByKey(platformKey, platforms);

  useEffect(() => {
    const detectedPlatform = detectDownloadPlatform();
    setPlatformKey(detectedPlatform.key);

    const userAgentData = (
      navigator as Navigator & {
        userAgentData?: {
          platform?: string;
          getHighEntropyValues?: (hints: string[]) => Promise<{architecture?: string; platform?: string}>;
        };
      }
    ).userAgentData;

    const entropyPromise = userAgentData?.getHighEntropyValues?.(['architecture', 'platform']);

    entropyPromise
      ?.then((values) => {
        const detectedOs = values.platform?.toLowerCase() ?? userAgentData?.platform?.toLowerCase() ?? '';
        const architecture = values.architecture?.toLowerCase() ?? '';

        if (detectedOs) {
          setPlatformKey(getPlatformKeyFromHints(detectedOs, architecture));
        }
      })
      .catch(() => {
        // The synchronous detector above is sufficient when high entropy hints are unavailable.
      });
  }, []);

  useEffect(() => {
    const controller = new AbortController();

    fetch(latestDownloadManifestUrl, {signal: controller.signal})
      .then((response) => {
        if (!response.ok) {
          throw new Error(`Failed to fetch latest download manifest: ${response.status}`);
        }

        return response.json() as Promise<LatestDownloadManifest>;
      })
      .then((manifest) => {
        setPlatforms(getDownloadPlatformsFromManifest(manifest));
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === 'AbortError') {
          return;
        }

        console.warn('Failed to load latest download manifest.', error);
      });

    return () => controller.abort();
  }, []);

  return (
    <div className={styles.downloadGroup}>
      <a className={clsx('button button--lg', styles.secondaryButton, styles.downloadPrimary)} href={platform.href}>
        <Translate>下载</Translate>
        <span className={styles.downloadPlatform}>{getDownloadPlatformLabel(platform.key)}</span>
      </a>
      <details className={styles.downloadMenu}>
        <summary className={styles.downloadMenuToggle} aria-label={translate({message: '选择下载平台'})}>
          <span />
        </summary>
        <div className={styles.downloadMenuList}>
          {platforms.map((item) => (
            <a key={item.key} className={styles.downloadMenuItem} href={item.href}>
              {getDownloadPlatformLabel(item.key)}
            </a>
          ))}
        </div>
      </details>
    </div>
  );
}

function FeaturePreview({
  title,
  lightImageUrl,
  darkImageUrl,
}: {
  title: string;
  lightImageUrl: string;
  darkImageUrl: string;
}) {
  const themeMode = useThemeMode();
  const targetImageUrl = themeMode === 'dark' ? darkImageUrl : lightImageUrl;
  const [displayedImageUrl, setDisplayedImageUrl] = useState(() =>
    isFeaturePreviewImageLoaded(targetImageUrl) ? targetImageUrl : '',
  );
  const [isLoading, setIsLoading] = useState(() => !isFeaturePreviewImageLoaded(targetImageUrl));

  useEffect(() => {
    let cancelled = false;

    if (!targetImageUrl) {
      setDisplayedImageUrl('');
      setIsLoading(false);
      return;
    }

    if (isFeaturePreviewImageLoaded(targetImageUrl)) {
      setDisplayedImageUrl(targetImageUrl);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);

    void preloadFeaturePreviewImage(targetImageUrl).then((loaded) => {
      if (cancelled) {
        return;
      }

      if (loaded) {
        setDisplayedImageUrl(targetImageUrl);
      }

      setIsLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [targetImageUrl]);

  return (
    <div className={styles.featurePreview} aria-label={title} aria-busy={isLoading}>
      <div className={clsx(styles.featurePreviewStage, isLoading && styles.featurePreviewStageLoading)}>
        {displayedImageUrl ? (
          <img
            key={displayedImageUrl}
            className={styles.featurePreviewImage}
            src={displayedImageUrl}
            alt={
              themeMode === 'dark'
                ? translate({message: 'NyaTerm 夜间主题功能截图'})
                : translate({message: 'NyaTerm 日间主题功能截图'})
            }
          />
        ) : (
          <div className={styles.featurePreviewPlaceholder} aria-hidden="true" />
        )}

        {isLoading ? (
          <div className={styles.featurePreviewLoadingOverlay} aria-hidden="true">
            <div className={styles.featurePreviewLoadingShimmer} />
            <div className={styles.featurePreviewLoadingGlass}>
              <span className={styles.featurePreviewSpinner} />
              <span className={styles.featurePreviewLoadingText}>
                <Translate>加载预览中</Translate>
              </span>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function HeroPreview({
  lightImage,
  darkImage,
}: {
  lightImage: string;
  darkImage: string;
}) {
  const lightImageUrl = useBaseUrl(lightImage);
  const darkImageUrl = useBaseUrl(darkImage);

  return (
    <div className={styles.heroPreview} aria-label={translate({message: '产品截图预览'})}>
      <div className={styles.heroScreenshotStage}>
        <div className={clsx(styles.heroScreenshotSlot, styles.heroScreenshotLight)}>
          <img
            className={styles.heroScreenshotImage}
            src={lightImageUrl}
            alt={translate({message: 'NyaTerm 日间主题产品截图'})}
          />
        </div>
        <div className={clsx(styles.heroScreenshotSlot, styles.heroScreenshotDark)}>
          <img
            className={styles.heroScreenshotImage}
            src={darkImageUrl}
            alt={translate({message: 'NyaTerm 夜间主题产品截图'})}
          />
        </div>
      </div>
    </div>
  );
}

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  const logoUrl = useBaseUrl('/img/logo.svg');

  const badges = [
    translate({message: 'SSH'}),
    translate({message: 'Local Shell'}),
    translate({message: 'Telnet / Serial'}),
    translate({message: 'SFTP'}),
    translate({message: 'WebDAV / S3'}),
  ];

  return (
    <header className={styles.heroBanner}>
      <div className="container">
        <div className={styles.heroInner}>
          <div className={styles.heroCopy}>
            <div className={styles.heroBrandRow}>
              <img src={logoUrl} alt="NyaTerm Logo" className={styles.heroLogo} />
              <div className={styles.heroBrandText}>
                <Heading as="h1" className={styles.heroTitle}>
                  {siteConfig.title}
                </Heading>
              </div>
            </div>

            <p className={styles.heroSubtitle}>
              <Translate>用于 SSH、串口和本地命令行工作的桌面终端客户端。</Translate>
            </p>
            <p className={styles.heroDescription}>
              <Translate>
                NyaTerm 将终端会话、远程文件、认证信息、端口转发和配置备份放在同一个桌面应用中，适合日常开发、服务器维护和设备调试。
              </Translate>
            </p>

            <div className={styles.heroButtons}>
              <Link
                className={clsx('button button--lg', styles.primaryButton)}
                to="/docs/getting-started/quick-start">
                <Translate>快速开始</Translate>
              </Link>
              <DownloadButton />
            </div>

            <div className={styles.heroBadges}>
              {badges.map((badge) => (
                <span key={badge} className={styles.heroBadge}>
                  {badge}
                </span>
              ))}
            </div>
          </div>

          <HeroPreview lightImage="/img/home/product-light.png" darkImage="/img/home/product-dark.png" />
        </div>
      </div>
    </header>
  );
}

function FeaturesSection({features}: {features: FeatureTab[]}) {
  const baseUrl = useBaseUrl('/');
  const resolvedFeatures = useMemo<FeatureTabWithUrls[]>(
    () =>
      features.map((feature) => ({
        ...feature,
        lightImageUrl: resolveAssetUrl(baseUrl, feature.lightImage),
        darkImageUrl: resolveAssetUrl(baseUrl, feature.darkImage),
      })),
    [baseUrl, features],
  );
  const [activeValue, setActiveValue] = useState(features[0]?.value ?? '');
  const [autoplayStopped, setAutoplayStopped] = useState(false);

  const activeIndex = Math.max(
    0,
    resolvedFeatures.findIndex((feature) => feature.value === activeValue),
  );
  const activeFeature = resolvedFeatures[activeIndex] ?? resolvedFeatures[0];
  const isAutoplaying = !autoplayStopped && resolvedFeatures.length > 1;

  const stopAutoplay = useCallback(() => {
    setAutoplayStopped(true);

    if (typeof sessionStorage !== 'undefined') {
      sessionStorage.setItem(featureAutoplayStoppedKey, 'true');
    }
  }, []);

  const selectFeature = useCallback(
    (value: string) => {
      stopAutoplay();
      setActiveValue(value);
    },
    [stopAutoplay],
  );

  const selectFeatureByOffset = useCallback(
    (offset: number) => {
      const nextIndex = (activeIndex + offset + resolvedFeatures.length) % resolvedFeatures.length;
      const nextValue = resolvedFeatures[nextIndex]?.value;

      if (nextValue) {
        selectFeature(nextValue);
      }
    },
    [activeIndex, resolvedFeatures, selectFeature],
  );

  useEffect(() => {
    if (typeof sessionStorage === 'undefined') {
      return;
    }

    setAutoplayStopped(sessionStorage.getItem(featureAutoplayStoppedKey) === 'true');
  }, []);

  useEffect(() => {
    if (!resolvedFeatures.some((feature) => feature.value === activeValue)) {
      setActiveValue(resolvedFeatures[0]?.value ?? '');
    }
  }, [activeValue, resolvedFeatures]);

  useEffect(() => {
    resolvedFeatures.forEach((feature) => {
      void preloadFeaturePreviewImage(feature.lightImageUrl);
      void preloadFeaturePreviewImage(feature.darkImageUrl);
    });
  }, [resolvedFeatures]);

  useEffect(() => {
    if (autoplayStopped) {
      return;
    }

    const mediaQuery = window.matchMedia?.('(prefers-reduced-motion: reduce)');
    if (mediaQuery?.matches) {
      setAutoplayStopped(true);
      return;
    }

    const stopOnUserAction = () => stopAutoplay();
    const listenerOptions: AddEventListenerOptions = {passive: true};

    window.addEventListener('pointerdown', stopOnUserAction, listenerOptions);
    window.addEventListener('keydown', stopOnUserAction);
    window.addEventListener('wheel', stopOnUserAction, listenerOptions);
    window.addEventListener('touchstart', stopOnUserAction, listenerOptions);

    return () => {
      window.removeEventListener('pointerdown', stopOnUserAction);
      window.removeEventListener('keydown', stopOnUserAction);
      window.removeEventListener('wheel', stopOnUserAction);
      window.removeEventListener('touchstart', stopOnUserAction);
    };
  }, [autoplayStopped, stopAutoplay]);

  useEffect(() => {
    if (!isAutoplaying) {
      return;
    }

    const intervalId = window.setInterval(() => {
      setActiveValue((currentValue) => {
        const currentIndex = resolvedFeatures.findIndex((feature) => feature.value === currentValue);
        const nextIndex = currentIndex >= 0 ? (currentIndex + 1) % resolvedFeatures.length : 0;
        return resolvedFeatures[nextIndex]?.value ?? currentValue;
      });
    }, featureAutoplayIntervalMs);

    return () => window.clearInterval(intervalId);
  }, [isAutoplaying, resolvedFeatures]);

  if (!activeFeature) {
    return null;
  }

  return (
    <section id="features" className={styles.featuresSection}>
      <div className="container">
        <div className={styles.featureShowcase}>
          <div className={styles.featureTabs} role="tablist" aria-label={translate({message: '功能展示'})}>
            {resolvedFeatures.map((feature, index) => {
              const isActive = feature.value === activeFeature.value;

              return (
                <button
                  key={feature.value}
                  type="button"
                  role="tab"
                  id={`feature-tab-${feature.value}`}
                  aria-controls={`feature-panel-${feature.value}`}
                  aria-selected={isActive}
                  className={clsx(styles.featureTab, isActive && styles.featureTabActive)}
                  onClick={() => selectFeature(feature.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
                      event.preventDefault();
                      selectFeatureByOffset(1);
                    }

                    if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
                      event.preventDefault();
                      selectFeatureByOffset(-1);
                    }

                    if (event.key === 'Home') {
                      event.preventDefault();
                      selectFeature(resolvedFeatures[0]?.value ?? feature.value);
                    }

                    if (event.key === 'End') {
                      event.preventDefault();
                      selectFeature(resolvedFeatures[resolvedFeatures.length - 1]?.value ?? feature.value);
                    }
                  }}>
                  <span className={styles.featureTabIndex}>{String(index + 1).padStart(2, '0')}</span>
                  <span className={styles.featureTabText}>
                    <span className={styles.featureTabLabel}>{feature.label}</span>
                    <span className={styles.featureTabHint}>{feature.title}</span>
                  </span>
                  <span className={styles.featureTabArrow} aria-hidden="true" />
                  {isActive && isAutoplaying ? <span className={styles.featureTabProgress} /> : null}
                </button>
              );
            })}
          </div>

          <div
            role="tabpanel"
            id={`feature-panel-${activeFeature.value}`}
            aria-labelledby={`feature-tab-${activeFeature.value}`}
            className={styles.featurePanel}>
            <div className={styles.featureCopy}>
              <Heading as="h2" className={styles.featureTitle}>
                {activeFeature.title}
              </Heading>
              <p className={styles.featureDescription}>{activeFeature.description}</p>
              <ul className={styles.featureList}>
                {activeFeature.bullets.map((bullet) => (
                  <li key={bullet} className={styles.featureListItem}>
                    {bullet}
                  </li>
                ))}
              </ul>
            </div>

            <FeaturePreview
              title={activeFeature.title}
              lightImageUrl={activeFeature.lightImageUrl}
              darkImageUrl={activeFeature.darkImageUrl}
            />
          </div>
        </div>
      </div>
    </section>
  );
}

function JourneySection() {
  const steps = [
    {
      title: translate({message: '安装并建立第一个连接'}),
      description: translate({message: '了解安装方式、创建连接和进入工作区的基本流程。'}),
      to: '/docs/getting-started/quick-start',
      label: translate({message: '查看快速开始'}),
    },
    {
      title: translate({message: '按功能查阅使用说明'}),
      description: translate({message: '查看 SSH、RDP/VNC、SFTP、终端操作、AI 助手、OTP、代理和云同步的具体用法。'}),
      to: '/docs/',
      label: translate({message: '查看文档'}),
    },
    {
      title: translate({message: '查看版本变化'}),
      description: translate({message: '了解各版本新增功能、行为调整和需要注意的兼容性变化。'}),
      to: '/changelog',
      label: translate({message: '查看更新记录'}),
    },
  ];

  return (
    <section className={styles.journeySection}>
      <div className="container">
        <div className={styles.sectionHeading}>
          <Heading as="h2" className={styles.sectionTitle}>
            <Translate>继续了解 NyaTerm</Translate>
          </Heading>
        </div>

        <div className={styles.journeyGrid}>
          {steps.map((step) => (
            <article key={step.title} className={styles.journeyCard}>
              <Heading as="h3" className={styles.journeyTitle}>
                {step.title}
              </Heading>
              <p className={styles.journeyDescription}>{step.description}</p>
              <Link className={styles.inlineLink} to={step.to}>
                {step.label}
              </Link>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

export default function Home(): React.ReactElement {
  const featureTabs: FeatureTab[] = [
    {
      value: 'workspace',
      label: translate({message: '工作区'}),
      title: translate({message: '在一个工作区中组织多种会话'}),
      description: translate({message: 'SSH、本地 shell、Telnet、串口、RDP 和 VNC 会话使用同一套标签页与分屏布局，便于同时查看不同主机、设备或任务。'}),
      bullets: [
        translate({message: '用标签页区分不同连接、任务或环境'}),
        translate({message: '在单个标签页内横向或纵向拆分终端区域'}),
        translate({message: '在侧边区域查看会话列表、文件和资源信息'}),
      ],
      lightImage: '/img/home/overview-light.png',
      darkImage: '/img/home/overview-dark.png',
    },
    {
      value: 'appearance',
      label: translate({message: '外观'}),
      title: translate({message: '用背景图定制主窗口氛围'}),
      description: translate({message: '主窗口支持本地背景图，可在保留面板可读性的同时营造更个性化的工作区视觉效果。'}),
      bullets: [
        translate({message: '使用本地图片作为主窗口的 Background Image'}),
        translate({message: '通过 cover、contain、stretch 和 tile 控制图片铺法'}),
        translate({message: '分别调整 Image Opacity 和 Background Content Opacity'}),
      ],
      lightImage: '/img/home/cover-light.png',
      darkImage: '/img/home/cover-dark.png',
    },
    {
      value: 'terminal',
      label: translate({message: '终端'}),
      title: translate({message: '改进命令输入和输出阅读'}),
      description: translate({message: '终端区域提供搜索、命令历史建议和输出标记，减少在长日志、重复命令和排错信息之间来回查找的成本。'}),
      bullets: [
        translate({message: '根据历史命令提供输入建议'}),
        translate({message: '支持终端内搜索、选中文本搜索和翻译'}),
        translate({message: '可显示时间戳、动作链接和关键词高亮'}),
      ],
      lightImage: '/img/home/terminal-light.png',
      darkImage: '/img/home/terminal-dark.png',
    },
    {
      value: 'files',
      label: translate({message: '文件传输'}),
      title: translate({message: '在终端旁处理远程文件'}),
      description: translate({message: 'SFTP 文件浏览器与终端工作区并列使用，适合查看日志、上传构建产物或调整远程配置文件。'}),
      bullets: [
        translate({message: '支持上传、下载、重命名、移动、删除和属性查看'}),
        translate({message: '传输任务可暂停、继续、取消和失败重试'}),
        translate({message: '本地编辑文件后可自动回传到远端路径'}),
      ],
      lightImage: '/img/home/files-light.png',
      darkImage: '/img/home/files-dark.png',
    },
    {
      value: 'security',
      label: translate({message: '安全与网络'}),
      title: translate({message: '管理连接认证和网络访问方式'}),
      description: translate({message: '围绕 SSH 连接所需的凭据、主机校验、一次性口令、代理和端口转发提供统一配置入口。'}),
      bullets: [
        translate({message: '保存密码、私钥和 known hosts，并使用本地加密存储'}),
        translate({message: '支持 TOTP / HOTP、二维码导入和 SSH 登录自动填充'}),
        translate({message: '配置代理、跳板机、本地转发、远程转发和动态转发'}),
      ],
      lightImage: '/img/home/security-light.png',
      darkImage: '/img/home/security-dark.png',
    },
    {
      value: 'sync',
      label: translate({message: '同步与备份'}),
      title: translate({message: '同步和恢复可移植配置'}),
      description: translate({message: 'NyaTerm 可将连接、凭据配置和常用设置打包为加密快照，用于备份、迁移或在多台设备之间同步。'}),
      bullets: [
        translate({message: '支持 WebDAV 和 S3 兼容存储'}),
        translate({message: '使用主密码保护同步和备份数据'}),
        translate({message: '支持 `.nya` 导入导出，并处理远端与本地版本冲突'}),
      ],
      lightImage: '/img/home/sync-light.png',
      darkImage: '/img/home/sync-dark.png',
    },
  ];

  return (
    <Layout
      title={translate({message: '首页'})}
      description={translate({message: '支持 SSH、RDP、VNC、串口、本地 shell、SFTP 和加密配置同步的原生桌面终端客户端'})}>
      <HomepageHeader />
      <main>
        <FeaturesSection features={featureTabs} />
        <JourneySection />
      </main>
    </Layout>
  );
}
