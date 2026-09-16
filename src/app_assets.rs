//! App-level asset source: gpui-kit's default icon set, plus a small set of
//! custom icons that the default set lacks (e.g. a cloud icon for cloud sync).

use std::borrow::Cow;

use gpui_kit::assets::Assets;
use gpui_kit::gpui::{AssetSource, Result, SharedString};

/// Lucide `cloud` icon, matching the stroke style of the default icon set.
const CLOUD_SVG: &[u8] = br#"<svg
    xmlns="http://www.w3.org/2000/svg"
    width="24"
    height="24"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="lucide lucide-cloud"
><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" /></svg>"#;

/// Custom icons are served by this key; the rest falls through to the default set.
const CLOUD_PATH: &str = "icons/cloud.svg";

pub struct AppAssets(Assets);

impl AppAssets {
    pub fn new() -> Self {
        Self(Assets::new(SharedString::default()))
    }
}

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == CLOUD_PATH {
            return Ok(Some(Cow::Borrowed(CLOUD_SVG)));
        }
        self.0.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut listed = self.0.list(path)?;
        if path.is_empty() || path == "icons" {
            if !listed.iter().any(|p| p.as_ref() == CLOUD_PATH) {
                listed.push(SharedString::from(CLOUD_PATH));
            }
        }
        Ok(listed)
    }
}
