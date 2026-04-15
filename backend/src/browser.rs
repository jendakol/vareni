//! Headless Chromium browser for rendering SPA pages.
//!
//! Provides [`launch`] to start a browser and [`fetch_html`] to get fully-rendered
//! HTML from a page that requires JavaScript execution.
//!
//! Includes stealth evasions (derived from puppeteer-extra-plugin-stealth) to bypass
//! headless browser detection on sites that check `navigator.webdriver`, plugins, etc.

use std::time::Duration;

use chromiumoxide::Browser;
use chromiumoxide::BrowserConfig;

/// What to wait for after navigation before extracting HTML.
#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Wait for a CSS selector to appear in the DOM.
    Selector(&'static str),
    /// Wait for no network activity for 500ms.
    /// Warning: modern sites with analytics/websockets may never reach idle.
    NetworkIdle,
}

/// Launch a headless Chromium instance with stealth flags.
///
/// Returns the `Browser` handle and a `JoinHandle` for the CDP connection
/// (chromiumoxide requires the connection future to be polled).
///
/// The browser process is killed when the `Browser` is dropped.
pub async fn launch() -> anyhow::Result<(Browser, tokio::task::JoinHandle<()>)> {
    let chrome_path = find_chrome()?;
    tracing::info!(path = %chrome_path, "Launching headless Chromium");

    let (browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .no_sandbox()
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-blink-features=AutomationControlled")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build browser config: {e}"))?,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to launch Chromium: {e}"))?;

    // Spawn a task to drive the CDP connection
    let handle = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    Ok((browser, handle))
}

/// Fetch fully-rendered HTML from a URL using a browser tab.
///
/// Opens a new tab, injects stealth evasions, navigates to `url`, waits per `wait`,
/// extracts `document.documentElement.outerHTML`, and closes the tab.
pub async fn fetch_html(
    browser: &Browser,
    url: &str,
    wait: &WaitCondition,
    timeout: Duration,
) -> anyhow::Result<String> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open new tab: {e}"))?;

    // Inject stealth evasions BEFORE any navigation
    let ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    if let Err(e) = page.enable_stealth_mode_with_agent(ua).await {
        tracing::warn!(error = %e, "Built-in stealth mode failed, continuing with manual evasions");
    }
    if let Err(e) = page.evaluate_on_new_document(STEALTH_JS).await {
        tracing::warn!(error = %e, "Stealth JS injection failed");
    }

    // Navigate and wait with timeout
    let result = tokio::time::timeout(timeout, async {
        tracing::debug!(url = %url, "Navigating browser tab");
        page.goto(url)
            .await
            .map_err(|e| anyhow::anyhow!("Navigation failed: {e}"))?;
        tracing::debug!(url = %url, "Navigation completed, waiting for content");

        // Wait for dynamic content using in-page JavaScript.
        // CDP's DOM.querySelector (used by find_element) can miss dynamically rendered content.
        // Running the wait inside the page's JS context sees the live DOM reliably.
        match wait {
            WaitCondition::Selector(sel) => {
                let js = format!(
                    r#"new Promise((resolve, reject) => {{
                        const timeout = setTimeout(() => reject('selector_timeout'), {});
                        const check = () => {{
                            if (document.querySelector('{}')) {{
                                clearTimeout(timeout);
                                resolve(true);
                            }} else {{
                                setTimeout(check, 200);
                            }}
                        }};
                        check();
                    }})"#,
                    // Use 80% of the Rust timeout for the JS wait, leave room for HTML extraction
                    (timeout.as_millis() as u64).saturating_sub(5000).max(5000),
                    sel.replace('\'', "\\'"),
                );
                match page.evaluate(js).await {
                    Ok(_) => tracing::debug!(url = %url, selector = %sel, "Selector found"),
                    Err(e) => tracing::warn!(url = %url, selector = %sel, error = %e, "Selector wait failed, extracting HTML anyway"),
                }
            }
            WaitCondition::NetworkIdle => {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }

        // Extract rendered HTML
        let html: String = page
            .evaluate("document.documentElement.outerHTML")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to extract HTML: {e}"))?
            .into_value()
            .map_err(|e| anyhow::anyhow!("Failed to deserialize HTML: {e}"))?;

        tracing::debug!(url = %url, html_len = html.len(), "Extracted HTML from browser");
        Ok::<String, anyhow::Error>(html)
    })
    .await;

    // Close the tab regardless of outcome
    let _ = page.close().await;

    match result {
        Ok(inner) => inner,
        Err(_) => anyhow::bail!("Page load timed out after {}s for {url}", timeout.as_secs()),
    }
}

/// Find the Chromium executable.
fn find_chrome() -> anyhow::Result<String> {
    if let Ok(path) = std::env::var("CHROME_PATH") {
        return Ok(path);
    }

    let candidates = [
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
    ];

    for path in candidates {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    anyhow::bail!(
        "Chromium not found. Set CHROME_PATH or install chromium. Searched: {}",
        candidates.join(", ")
    )
}

/// Stealth JavaScript evasions injected before page navigation.
/// Derived from puppeteer-extra-plugin-stealth to bypass headless detection.
const STEALTH_JS: &str = r#"(() => {
  // 1. navigator.webdriver — return undefined like real Chrome
  Object.defineProperty(Object.getPrototypeOf(navigator), 'webdriver', {
    get: () => undefined
  });

  // 2. window.chrome + chrome.app/csi/loadTimes/runtime
  if (!window.chrome) {
    Object.defineProperty(window, 'chrome', {
      writable: true, enumerable: true, configurable: false, value: {}
    });
  }
  if (!('app' in window.chrome)) {
    window.chrome.app = {
      isInstalled: false,
      InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' },
      RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' },
      get isInstalled() { return false; },
      getDetails: function() { return null; },
      getIsInstalled: function() { return false; },
      runningState: function() { return 'cannot_run'; }
    };
  }
  if (!('csi' in window.chrome)) {
    window.chrome.csi = function() {
      const t = performance.timing;
      return { onloadT: t.domContentLoadedEventEnd, startE: t.navigationStart, pageT: Date.now() - t.navigationStart, tran: 15 };
    };
  }
  if (!('loadTimes' in window.chrome)) {
    window.chrome.loadTimes = function() {
      const t = performance.timing;
      const n = (performance.getEntriesByType && performance.getEntriesByType('navigation')[0]) || {};
      return {
        get connectionInfo() { return n.nextHopProtocol || 'h2'; },
        get npnNegotiatedProtocol() { return ['h2','hq'].includes(n.nextHopProtocol||'') ? n.nextHopProtocol : 'unknown'; },
        get navigationType() { return n.type || 'other'; },
        get wasAlternateProtocolAvailable() { return false; },
        get wasFetchedViaSpdy() { return ['h2','hq'].includes(n.nextHopProtocol||''); },
        get wasNpnNegotiated() { return ['h2','hq'].includes(n.nextHopProtocol||''); },
        get requestTime() { return t.navigationStart / 1000; },
        get startLoadTime() { return t.navigationStart / 1000; },
        get commitLoadTime() { return t.responseStart / 1000; },
        get finishDocumentLoadTime() { return t.domContentLoadedEventEnd / 1000; },
        get finishLoadTime() { return t.loadEventEnd / 1000; },
        get firstPaintTime() { const fp = (performance.getEntriesByType&&performance.getEntriesByType('paint')||[])[0]; return fp ? (fp.startTime+performance.timeOrigin)/1000 : t.loadEventEnd/1000; },
        get firstPaintAfterLoadTime() { return 0; }
      };
    };
  }
  if (!window.chrome.runtime) {
    window.chrome.runtime = {
      OnInstalledReason: { CHROME_UPDATE:'chrome_update', INSTALL:'install', SHARED_MODULE_UPDATE:'shared_module_update', UPDATE:'update' },
      OnRestartRequiredReason: { APP_UPDATE:'app_update', OS_UPDATE:'os_update', PERIODIC:'periodic' },
      PlatformArch: { ARM:'arm', MIPS:'mips', MIPS64:'mips64', X86_32:'x86-32', X86_64:'x86-64' },
      PlatformOs: { ANDROID:'android', CROS:'cros', LINUX:'linux', MAC:'mac', OPENBSD:'openbsd', WIN:'win' },
      RequestUpdateCheckStatus: { NO_UPDATE:'no_update', THROTTLED:'throttled', UPDATE_AVAILABLE:'update_available' },
      get id() { return undefined; }, connect: null, sendMessage: null
    };
  }

  // 3. navigator.languages / vendor / hardwareConcurrency
  Object.defineProperty(Object.getPrototypeOf(navigator), 'languages', { get: () => Object.freeze(['cs','en-US','en']) });
  Object.defineProperty(Object.getPrototypeOf(navigator), 'vendor', { get: () => 'Google Inc.' });
  Object.defineProperty(Object.getPrototypeOf(navigator), 'hardwareConcurrency', { get: () => 4 });

  // 4. WebGL vendor/renderer
  const getParam = WebGLRenderingContext.prototype.getParameter;
  WebGLRenderingContext.prototype.getParameter = function(p) {
    if (p === 37445) return 'Google Inc. (NVIDIA)';
    if (p === 37446) return 'ANGLE (NVIDIA, NVIDIA GeForce GTX 1050 Direct3D11 vs_5_0 ps_5_0)';
    return getParam.call(this, p);
  };
  if (typeof WebGL2RenderingContext !== 'undefined') {
    const getParam2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(p) {
      if (p === 37445) return 'Google Inc. (NVIDIA)';
      if (p === 37446) return 'ANGLE (NVIDIA, NVIDIA GeForce GTX 1050 Direct3D11 vs_5_0 ps_5_0)';
      return getParam2.call(this, p);
    };
  }

  // 5. window.outerWidth/outerHeight
  try {
    if (!window.outerWidth || !window.outerHeight) {
      window.outerWidth = window.innerWidth;
      window.outerHeight = window.innerHeight + 85;
    }
  } catch(e) {}

  // 6. Patch Function.prototype.toString to hide overrides
  const origToString = Function.prototype.toString;
  const fnMap = new WeakMap();
  fnMap.set(WebGLRenderingContext.prototype.getParameter, 'function getParameter() { [native code] }');
  if (typeof WebGL2RenderingContext !== 'undefined') {
    fnMap.set(WebGL2RenderingContext.prototype.getParameter, 'function getParameter() { [native code] }');
  }
  Function.prototype.toString = function() {
    return fnMap.has(this) ? fnMap.get(this) : origToString.call(this);
  };
  fnMap.set(Function.prototype.toString, 'function toString() { [native code] }');
})();"#;
