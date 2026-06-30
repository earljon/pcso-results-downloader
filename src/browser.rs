use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::browser::{
    Bounds, GetWindowForTargetParams, SetWindowBoundsParams, WindowState,
};
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetLocaleOverrideParams, SetTimezoneOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::network::SetExtraHttpHeadersParams;
use chromiumoxide::Page;
use futures::StreamExt;
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::debug;

use crate::dates::month_name;
use crate::error::{PcsoError, Result};

const SEARCH_URL: &str = "https://www.pcso.gov.ph/SearchLottoResult.aspx";
const REFERER: &str = "https://www.pcso.gov.ph/";
const PAGE_TIMEOUT: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const CHALLENGE_SETTLE: Duration = Duration::from_secs(3);

// Matches the working Node/Playwright version on the Pi.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36";
const LOCALE: &str = "en-US";
const TIMEZONE: &str = "Asia/Manila";

/// Resolves the persistent Chromium profile directory.
///
/// Precedence:
///   1. `--profile-dir <PATH>` CLI flag (caller passes it in)
///   2. `PCSO_PROFILE_DIR` env var
///   3. `.pcso-profile/` next to the binary itself
///
/// Keeping cookies between runs lets Akamai's JS challenge results stick —
/// the single biggest factor for headless to keep working against PCSO.
pub fn resolve_profile_dir(cli_override: Option<PathBuf>) -> PathBuf {
    if let Some(p) = cli_override {
        return p;
    }
    if let Ok(env_p) = std::env::var("PCSO_PROFILE_DIR") {
        if !env_p.is_empty() {
            return PathBuf::from(env_p);
        }
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join(".pcso-profile")
}

pub async fn launch(
    headed: bool,
    minimize: bool,
    profile: PathBuf,
) -> Result<(Browser, JoinHandle<()>)> {
    if let Err(e) = std::fs::create_dir_all(&profile) {
        return Err(PcsoError::Browser(format!(
            "create profile dir {}: {e}",
            profile.display()
        )));
    }

    // Match Playwright's exact launch flags as captured from the working
    // Pi/Node setup. Two key things that gate Akamai access:
    //   1) OLD headless (--headless), not --headless=new. Akamai's WAF blocks
    //      new-headless on pcso.gov.ph but allows the old/legacy mode.
    //   2) The full --disable-features list that turns off the features Akamai
    //      fingerprints (AcceptCHFrame, PaintHolding, ThirdPartyStoragePartitioning, …).
    // chromiumoxide's DEFAULT_ARGS uses an incompatible set, so we disable
    // defaults and re-add the exact Playwright flag set.
    // IMPORTANT: chromiumoxide's .arg() takes the flag NAME without `--` and
    // prepends `--` itself. Passing "--disable-popup-blocking" produces the
    // invalid `----disable-popup-blocking` (which Chromium silently ignores).
    // Use bare keys for boolean flags and tuple form `(key, value)` for
    // key=value flags.
    //
    // Default headless mode is HeadlessMode::True (old --headless), which is
    // what Akamai's WAF for pcso.gov.ph allows. Old headless also auto-emits
    // --hide-scrollbars and --mute-audio, so those aren't repeated below.
    let mut builder = BrowserConfig::builder()
        .window_size(1366, 900)
        .user_data_dir(&profile)
        .no_sandbox()
        .disable_default_args()
        .arg(("user-agent", USER_AGENT))
        .arg(("lang", LOCALE))
        .arg("no-first-run")
        .arg("no-default-browser-check")
        .arg("no-service-autorun")
        .arg("disable-field-trial-config")
        .arg("disable-background-networking")
        .arg("disable-background-timer-throttling")
        .arg("disable-backgrounding-occluded-windows")
        .arg("disable-back-forward-cache")
        .arg("disable-breakpad")
        .arg("disable-client-side-phishing-detection")
        .arg("disable-component-extensions-with-background-pages")
        .arg("disable-component-update")
        .arg("disable-default-apps")
        .arg("disable-dev-shm-usage")
        .arg("disable-extensions")
        .arg("disable-hang-monitor")
        .arg("disable-ipc-flooding-protection")
        .arg("disable-popup-blocking")
        .arg("disable-prompt-on-repost")
        .arg("disable-renderer-backgrounding")
        .arg("disable-search-engine-choice-screen")
        .arg("disable-pings")
        .arg(("media-router", "0"))
        .arg("export-tagged-pdf")
        .arg("unsafely-disable-devtools-self-xss-warnings")
        .arg(("force-color-profile", "srgb"))
        .arg("metrics-recording-only")
        .arg(("password-store", "basic"))
        .arg("use-mock-keychain")
        .arg("allow-pre-commit-input")
        .arg("enable-automation")
        .arg((
            "blink-settings",
            "primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4",
        ))
        .arg((
            "disable-features",
            "AcceptCHFrame,AvoidUnnecessaryBeforeUnloadCheckSync,DestroyProfileOnBrowserClose,DialMediaRouteProvider,GlobalMediaControls,HttpsUpgrades,LensOverlay,MediaRouter,PaintHolding,ThirdPartyStoragePartitioning,Translate,AutoDeElevate",
        ));
    if headed {
        builder = builder.with_head();
        // Park the window far off-screen so it never visually appears or steals
        // focus on the active display. CDP setWindowBounds (below) makes this
        // permanent; the command-line hint covers the brief window between
        // process spawn and our first CDP call.
        if minimize {
            builder = builder
                .arg(("window-position", "-32000,-32000"))
                .arg(("window-size", "1,1"))
                // Skip the initial blank window entirely; CDP will create pages
                // on demand, which we immediately reposition off-screen.
                .arg("no-startup-window");
        }
    }
    let config = builder
        .build()
        .map_err(|e| PcsoError::Browser(format!("browser config: {e}")))?;

    let (browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| PcsoError::Browser(format!("launch chrome: {e}")))?;

    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                debug!("chromiumoxide handler event: {e}");
            }
        }
    });

    debug!("using persistent profile at {}", profile.display());

    if minimize && headed {
        if let Err(e) = minimize_window(&browser).await {
            debug!("could not minimize window: {e}");
        }
    }

    Ok((browser, handle))
}

async fn minimize_window(browser: &Browser) -> Result<()> {
    // Need a target/page to ask for its window id.
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| PcsoError::Browser(format!("new_page for minimize: {e}")))?;

    let win = page
        .execute(GetWindowForTargetParams::default())
        .await
        .map_err(|e| PcsoError::Browser(format!("get window: {e}")))?;

    // First park the window far off-screen (CDP bypasses the on-screen clamping
    // that the --window-position command-line flag is subject to on macOS).
    let off_bounds = Bounds::builder()
        .left(-32000)
        .top(-32000)
        .width(1)
        .height(1)
        .window_state(WindowState::Normal)
        .build();
    let off_params = SetWindowBoundsParams::builder()
        .window_id(win.result.window_id)
        .bounds(off_bounds)
        .build()
        .map_err(|e| PcsoError::Browser(format!("build off-screen bounds: {e}")))?;
    let _ = page.execute(off_params).await;

    // Then ask the OS to minimize, so it doesn't even show in mission control.
    let min_bounds = Bounds::builder().window_state(WindowState::Minimized).build();
    let min_params = SetWindowBoundsParams::builder()
        .window_id(win.result.window_id)
        .bounds(min_bounds)
        .build()
        .map_err(|e| PcsoError::Browser(format!("build minimized bounds: {e}")))?;
    let _ = page.execute(min_params).await;

    let _ = page.close().await;
    Ok(())
}

pub async fn fetch_result_html(browser: &Browser, date: NaiveDate) -> Result<String> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| PcsoError::Browser(format!("new_page: {e}")))?;

    // Hide the webdriver flag before any navigation runs.
    let _ = page
        .evaluate_on_new_document(
            "Object.defineProperty(navigator,'webdriver',{get:()=>undefined});",
        )
        .await;

    apply_stealth_overrides(&page).await?;

    page.goto(SEARCH_URL)
        .await
        .map_err(|e| PcsoError::Browser(format!("goto: {e}")))?;

    // Give Akamai's JS challenge a moment to run and set its cookies.
    sleep(CHALLENGE_SETTLE).await;

    wait_until_true(
        &page,
        r#"!!document.querySelector('select') &&
           Array.from(document.querySelectorAll('input,button'))
                .some(b => (b.value || b.innerText || '').trim() === 'Search Lotto')"#,
        "initial form to render",
    )
    .await?;

    fill_form_and_submit(&page, date).await?;

    let html = page
        .content()
        .await
        .map_err(|e| PcsoError::Browser(format!("page.content: {e}")))?;

    let _ = page.close().await;

    Ok(html)
}

async fn apply_stealth_overrides(page: &Page) -> Result<()> {
    let mut headers = HashMap::new();
    headers.insert(
        "Accept".to_string(),
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".to_string(),
    );
    headers.insert("Accept-Language".to_string(), "en-US,en;q=0.9".to_string());
    headers.insert("Referer".to_string(), REFERER.to_string());

    let headers_value = serde_json::to_value(headers)
        .map_err(|e| PcsoError::Browser(format!("serialize headers: {e}")))?;
    page.execute(SetExtraHttpHeadersParams::new(
        chromiumoxide::cdp::browser_protocol::network::Headers::new(headers_value),
    ))
    .await
    .map_err(|e| PcsoError::Browser(format!("set extra headers: {e}")))?;

    page.execute(SetLocaleOverrideParams {
        locale: Some(LOCALE.to_string()),
    })
    .await
    .map_err(|e| PcsoError::Browser(format!("set locale: {e}")))?;

    page.execute(SetTimezoneOverrideParams {
        timezone_id: TIMEZONE.to_string(),
    })
    .await
    .map_err(|e| PcsoError::Browser(format!("set timezone: {e}")))?;

    Ok(())
}

async fn wait_until_true(page: &Page, predicate_js: &str, what: &str) -> Result<()> {
    let deadline = Instant::now() + PAGE_TIMEOUT;
    let script = format!("(() => {{ try {{ return ({predicate_js}); }} catch (_) {{ return false; }} }})()");
    let mut last_log = Instant::now();
    loop {
        match page.evaluate(script.as_str()).await {
            Ok(eval) => {
                if eval.value().and_then(Value::as_bool) == Some(true) {
                    return Ok(());
                }
                if last_log.elapsed() >= Duration::from_secs(3) {
                    debug!("still waiting for {what}: predicate=false");
                    last_log = Instant::now();
                }
            }
            Err(e) => debug!("wait_until_true poll error ({what}): {e}"),
        }
        if Instant::now() >= deadline {
            if let Ok(peek) = page
                .evaluate(
                    "(() => ({ url: location.href, ready: document.readyState, \
                      selects: document.querySelectorAll('select').length, \
                      buttons: document.querySelectorAll('input,button').length, \
                      title: document.title }))()",
                )
                .await
            {
                debug!("diagnosis on timeout ({what}): {:?}", peek.value());
            }
            return Err(PcsoError::Timeout(what.to_string()));
        }
        sleep(POLL_INTERVAL).await;
    }
}

async fn fill_form_and_submit(page: &Page, date: NaiveDate) -> Result<()> {
    let month = month_name(date);
    let day = date.day().to_string();
    let year = date.year().to_string();

    let set_script = format!(
        r#"
        (() => {{
            const all = Array.from(document.querySelectorAll('select'));
            const isMonth = s => Array.from(s.options).some(o => o.text.trim() === 'January')
                              && Array.from(s.options).some(o => o.text.trim() === 'December');
            const isDay   = s => Array.from(s.options).some(o => o.text.trim() === '1')
                              && Array.from(s.options).some(o => o.text.trim() === '31');
            const isYear  = s => Array.from(s.options).some(o => o.text.trim() === '2026')
                              && Array.from(s.options).some(o => /^20[12]\d$/.test(o.text.trim()));
            const months = all.filter(isMonth);
            const days   = all.filter(isDay);
            const years  = all.filter(isYear);

            const setAll = (els, value) => {{
                if (els.length === 0) return false;
                for (const el of els) {{
                    const opt = Array.from(el.options).find(o =>
                        o.value === value || o.text.trim() === value
                    );
                    if (!opt) return false;
                    el.value = opt.value;
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                }}
                return true;
            }};

            return {{
                month: setAll(months, {month:?}),
                day:   setAll(days,   {day:?}),
                year:  setAll(years,  {year:?}),
                counts: {{ months: months.length, days: days.length, years: years.length }},
            }};
        }})()
        "#,
        month = month, day = day, year = year,
    );

    let result = page
        .evaluate(set_script.as_str())
        .await
        .map_err(|e| PcsoError::Browser(format!("set form values: {e}")))?;
    let ok = match result.value() {
        Some(v) => {
            v.get("month").and_then(Value::as_bool) == Some(true)
                && v.get("day").and_then(Value::as_bool) == Some(true)
                && v.get("year").and_then(Value::as_bool) == Some(true)
        }
        None => false,
    };
    if !ok {
        return Err(PcsoError::Browser(format!(
            "could not set all date dropdowns (result={:?})",
            result.value()
        )));
    }

    page.evaluate("document.documentElement.setAttribute('data-pcso-pre','1')")
        .await
        .map_err(|e| PcsoError::Browser(format!("tag page: {e}")))?;

    let clicker = r#"
        (() => {
            const btn = Array.from(document.querySelectorAll('input,button'))
                .find(b => (b.value || b.innerText || '').trim() === 'Search Lotto');
            if (!btn) return false;
            btn.click();
            return true;
        })()
    "#;
    let clicked = page
        .evaluate(clicker)
        .await
        .map_err(|e| PcsoError::Browser(format!("click submit: {e}")))?;
    if clicked.value().and_then(Value::as_bool) != Some(true) {
        return Err(PcsoError::Browser("Search Lotto button not found".into()));
    }

    wait_until_true(
        page,
        "!document.documentElement.hasAttribute('data-pcso-pre') \
         && Array.from(document.querySelectorAll('input,button')) \
              .some(b => (b.value || b.innerText || '').trim() === 'Search Lotto')",
        "postback to complete",
    )
    .await?;

    sleep(Duration::from_millis(500)).await;

    Ok(())
}
