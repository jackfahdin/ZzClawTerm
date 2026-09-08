//! Classification and size-limit policy for the built-in remote file preview.
//!
//! This mirrors the categories and byte ceilings the Tauri build applied before
//! deciding whether a remote file can be previewed and how it should be
//! rendered. It is deliberately UI-independent: the desktop crate maps the
//! resulting [`PreviewCategory`] onto a GPUI view, but the decision of *what a
//! file is* and *whether it is small enough to fetch* lives here so it can be
//! tested without a window.
//!
//! Parity note: the Tauri client offered a "Preview" action for **every**
//! non-directory file and, when the format was not one it could render, opened
//! the preview window and showed an "unsupported format" message *without*
//! fetching the bytes. [`classify_preview`] therefore always returns a category
//! for a file name; [`PreviewCategory::Unsupported`] is the terminal state that
//! renders that message and is never fetched. Only a directory has no category.
//!
//! The ceilings are compatibility contracts with the previous client, so
//! changing them changes which files a user can preview. Keep them in step with
//! the documented limits (text 5 MiB, CSV/TSV 10 MiB, image/PDF 25 MiB).

/// One mebibyte, the unit the ceilings below are expressed in.
const MIB: u64 = 1024 * 1024;

/// Largest text/JSON/Markdown payload the preview will fetch and render.
pub const PREVIEW_TEXT_MAX_BYTES: u64 = 5 * MIB;

/// Largest delimited (CSV/TSV) payload the preview will fetch and render.
///
/// Higher than plain text because tabular exports are routinely larger than
/// source or config files and still cheap to render as a grid.
pub const PREVIEW_CSV_MAX_BYTES: u64 = 10 * MIB;

/// Largest image or PDF payload the preview will fetch before decoding.
///
/// Decoding happens off the UI thread, but the ceiling bounds both the transfer
/// and the peak decode allocation, so it applies to the raw file rather than the
/// decoded surface.
pub const PREVIEW_IMAGE_MAX_BYTES: u64 = 25 * MIB;

/// Largest PDF payload the preview will fetch before rasterizing.
pub const PREVIEW_PDF_MAX_BYTES: u64 = 25 * MIB;

/// What kind of viewer a remote file maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCategory {
    /// Plain source, config or log text rendered read-only.
    Text,
    /// JSON, rendered as pretty-printed read-only text.
    Json,
    /// Markdown, rendered through the shared markdown block renderer.
    Markdown,
    /// Delimited data rendered as a read-only grid. `tab` selects the delimiter.
    Delimited { tab: bool },
    /// A raster image decoded and shown with zoom/rotate controls.
    Image,
    /// A PDF rasterized page-by-page.
    Pdf,
    /// A non-directory file whose format the preview cannot render. The window
    /// still opens and shows an "unsupported format" message, but no bytes are
    /// fetched. Matches the Tauri behaviour of previewing every file.
    Unsupported,
}

impl PreviewCategory {
    /// The byte ceiling that applies to this category.
    ///
    /// [`PreviewCategory::Unsupported`] reports `0` because it is never fetched;
    /// callers gate the fetch on [`PreviewCategory::is_fetchable`] first.
    pub fn max_bytes(self) -> u64 {
        match self {
            PreviewCategory::Text | PreviewCategory::Json | PreviewCategory::Markdown => {
                PREVIEW_TEXT_MAX_BYTES
            }
            PreviewCategory::Delimited { .. } => PREVIEW_CSV_MAX_BYTES,
            PreviewCategory::Image => PREVIEW_IMAGE_MAX_BYTES,
            PreviewCategory::Pdf => PREVIEW_PDF_MAX_BYTES,
            PreviewCategory::Unsupported => 0,
        }
    }

    /// Whether the category is fetched as raw bytes (image/PDF) rather than text.
    pub fn is_binary(self) -> bool {
        matches!(self, PreviewCategory::Image | PreviewCategory::Pdf)
    }

    /// Whether opening this category triggers a remote read at all.
    ///
    /// [`PreviewCategory::Unsupported`] is not fetched: the window opens and
    /// renders the unsupported message directly.
    pub fn is_fetchable(self) -> bool {
        !matches!(self, PreviewCategory::Unsupported)
    }
}

/// Lowercase extension of `name`, without the dot; empty when there is none.
fn preview_extension(name: &str) -> String {
    let normalized = name.trim().to_ascii_lowercase();
    let base = normalized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(normalized.as_str());
    match base.rfind('.') {
        // A leading dot is a hidden file, not an extension (`.gitignore`).
        Some(index) if index > 0 => base[index + 1..].to_string(),
        _ => String::new(),
    }
}

/// Lowercase final path component of `name`.
fn preview_basename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}

/// Which preview viewer a file name maps to.
///
/// Returns `None` only for directories (callers pass those separately). Every
/// non-directory file resolves to a category: a recognised format maps to its
/// renderer, and everything else maps to [`PreviewCategory::Unsupported`], which
/// opens the window and shows the unsupported message without fetching.
///
/// The recognised sets are an exact mirror of the Tauri `model.ts`
/// classification:
///
/// * image: `png` `jpg` `jpeg` `gif` `webp` `bmp`
/// * markdown: `md` `markdown` `mdx`
/// * CSV: `csv` `tsv`
/// * JSON: `json` `jsonc` `json5`
/// * PDF: `pdf`
/// * everything else that [`is_known_text_file`] recognises: text
pub fn classify_preview(name: &str) -> PreviewCategory {
    let extension = preview_extension(name);
    match extension.as_str() {
        "json" | "jsonc" | "json5" => return PreviewCategory::Json,
        "md" | "markdown" | "mdx" => return PreviewCategory::Markdown,
        "csv" => return PreviewCategory::Delimited { tab: false },
        "tsv" => return PreviewCategory::Delimited { tab: true },
        "pdf" => return PreviewCategory::Pdf,
        _ => {}
    }
    if is_preview_image_extension(&extension) {
        return PreviewCategory::Image;
    }
    if is_known_text_file(name) {
        return PreviewCategory::Text;
    }
    PreviewCategory::Unsupported
}

/// Whether a preview of `size` bytes is within the ceiling for `category`.
pub fn preview_within_limit(category: PreviewCategory, size: u64) -> bool {
    size <= category.max_bytes()
}

fn is_preview_image_extension(extension: &str) -> bool {
    // Exact mirror of the Tauri image set. Only formats the bundled `image`
    // decoder supports without extra features; an ".ico"/".tiff" file would
    // otherwise fail to decode after a full fetch and is treated as unsupported.
    matches!(extension, "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
}

/// Whether `name` is a plain-text file the preview can render as text.
///
/// Mirrors the Tauri `isKnownTextFile` helper: a broad set of source, config,
/// and log extensions, plus a handful of well-known extensionless files.
pub fn is_known_text_file(name: &str) -> bool {
    let extension = preview_extension(name);
    is_known_text_extension(&extension) || is_known_text_basename(name)
}

fn is_known_text_extension(extension: &str) -> bool {
    matches!(
        extension,
        "asc"
            | "bash"
            | "bat"
            | "c"
            | "cc"
            | "cfg"
            | "cjs"
            | "cmd"
            | "conf"
            | "cpp"
            | "cs"
            | "css"
            | "cxx"
            | "dart"
            | "diff"
            | "env"
            | "fish"
            | "go"
            | "h"
            | "hpp"
            | "htm"
            | "html"
            | "ini"
            | "java"
            | "js"
            | "jsx"
            | "kt"
            | "kts"
            | "log"
            | "lua"
            | "mjs"
            | "patch"
            | "pem"
            | "php"
            | "pl"
            | "properties"
            | "proto"
            | "ps1"
            | "py"
            | "r"
            | "rb"
            | "rs"
            | "sass"
            | "scss"
            | "service"
            | "sh"
            | "socket"
            | "sql"
            | "swift"
            | "timer"
            | "toml"
            | "ts"
            | "tsx"
            | "txt"
            | "vue"
            | "xml"
            | "yaml"
            | "yml"
            | "zsh"
    )
}

fn is_known_text_basename(name: &str) -> bool {
    let base_name = preview_basename(name);
    let normalized = base_name.trim_start_matches('.');
    matches!(
        normalized,
        "bash_profile"
            | "bash_login"
            | "bash_logout"
            | "bashrc"
            | "cmakelists.txt"
            | "dockerfile"
            | "editorconfig"
            | "env"
            | "env.local"
            | "gitconfig"
            | "gitignore"
            | "gitmodules"
            | "gitattributes"
            | "makefile"
            | "gnumakefile"
            | "npmrc"
            | "profile"
            | "zprofile"
            | "zshenv"
            | "zshrc"
    ) || base_name.ends_with(".dockerfile")
        || base_name.ends_with(".nginx.conf")
        || base_name == "docker-compose.yml"
        || base_name == "docker-compose.yaml"
}

#[cfg(test)]
mod tests {
    use super::{
        PREVIEW_CSV_MAX_BYTES, PREVIEW_IMAGE_MAX_BYTES, PREVIEW_PDF_MAX_BYTES,
        PREVIEW_TEXT_MAX_BYTES, PreviewCategory, classify_preview, is_known_text_file,
        preview_within_limit,
    };

    #[test]
    fn structured_text_categories_are_distinct_from_plain_text() {
        assert_eq!(classify_preview("data.json"), PreviewCategory::Json);
        assert_eq!(classify_preview("config.jsonc"), PreviewCategory::Json);
        assert_eq!(classify_preview("config.json5"), PreviewCategory::Json);
        assert_eq!(classify_preview("README.md"), PreviewCategory::Markdown);
        assert_eq!(classify_preview("doc.markdown"), PreviewCategory::Markdown);
        assert_eq!(classify_preview("page.mdx"), PreviewCategory::Markdown);
        assert_eq!(
            classify_preview("rows.csv"),
            PreviewCategory::Delimited { tab: false }
        );
        assert_eq!(
            classify_preview("rows.tsv"),
            PreviewCategory::Delimited { tab: true }
        );
        assert_eq!(classify_preview("main.rs"), PreviewCategory::Text);
    }

    #[test]
    fn extensions_match_tauri_model_exactly_without_extras() {
        // The old GPUI classifier accepted these; Tauri model.ts does not, so
        // they must now fall back to plain text (mdown/mkd) or be re-split.
        assert_eq!(
            classify_preview("notes.mdown"),
            PreviewCategory::Unsupported
        );
        assert_eq!(classify_preview("notes.mkd"), PreviewCategory::Unsupported);
        // `.tab` was an extra TSV alias that Tauri never had.
        assert_eq!(classify_preview("data.tab"), PreviewCategory::Unsupported);
    }

    #[test]
    fn images_and_pdf_map_to_binary_categories() {
        for name in ["photo.png", "photo.JPG", "sprite.webp", "anim.gif", "b.bmp"] {
            assert_eq!(classify_preview(name), PreviewCategory::Image, "{name}");
        }
        assert_eq!(classify_preview("report.pdf"), PreviewCategory::Pdf);
        assert!(PreviewCategory::Image.is_binary());
        assert!(PreviewCategory::Pdf.is_binary());
        assert!(!PreviewCategory::Text.is_binary());
    }

    #[test]
    fn extensionless_config_and_dotfiles_preview_as_text() {
        assert_eq!(classify_preview("Dockerfile"), PreviewCategory::Text);
        assert_eq!(classify_preview("Makefile"), PreviewCategory::Text);
        assert_eq!(classify_preview(".bashrc"), PreviewCategory::Text);
        assert_eq!(
            classify_preview("/etc/nginx/app.nginx.conf"),
            PreviewCategory::Text
        );
        assert!(is_known_text_file("main.rs"));
        assert!(!is_known_text_file("archive.zip"));
    }

    #[test]
    fn unrecognised_files_map_to_unsupported_not_none() {
        // Parity: every non-directory file previews. Unrenderable ones open the
        // window and show the unsupported message instead of being hidden.
        assert_eq!(
            classify_preview("archive.zip"),
            PreviewCategory::Unsupported
        );
        assert_eq!(classify_preview("clip.mp4"), PreviewCategory::Unsupported);
        assert_eq!(classify_preview("libfoo.so"), PreviewCategory::Unsupported);
        assert_eq!(classify_preview("icon.ico"), PreviewCategory::Unsupported);
        assert_eq!(classify_preview("scan.tiff"), PreviewCategory::Unsupported);
        assert!(!PreviewCategory::Unsupported.is_fetchable());
        assert!(PreviewCategory::Text.is_fetchable());
        assert_eq!(PreviewCategory::Unsupported.max_bytes(), 0);
    }

    #[test]
    fn limits_match_documented_ceilings_per_category() {
        assert_eq!(PreviewCategory::Text.max_bytes(), PREVIEW_TEXT_MAX_BYTES);
        assert_eq!(PreviewCategory::Json.max_bytes(), PREVIEW_TEXT_MAX_BYTES);
        assert_eq!(
            PreviewCategory::Markdown.max_bytes(),
            PREVIEW_TEXT_MAX_BYTES
        );
        assert_eq!(
            PreviewCategory::Delimited { tab: false }.max_bytes(),
            PREVIEW_CSV_MAX_BYTES
        );
        assert_eq!(PreviewCategory::Image.max_bytes(), PREVIEW_IMAGE_MAX_BYTES);
        assert_eq!(PreviewCategory::Pdf.max_bytes(), PREVIEW_PDF_MAX_BYTES);

        assert_eq!(PREVIEW_TEXT_MAX_BYTES, 5 * 1024 * 1024);
        assert_eq!(PREVIEW_CSV_MAX_BYTES, 10 * 1024 * 1024);
        assert_eq!(PREVIEW_IMAGE_MAX_BYTES, 25 * 1024 * 1024);
        assert_eq!(PREVIEW_PDF_MAX_BYTES, 25 * 1024 * 1024);
    }

    #[test]
    fn within_limit_is_inclusive_of_the_ceiling() {
        assert!(preview_within_limit(
            PreviewCategory::Text,
            PREVIEW_TEXT_MAX_BYTES
        ));
        assert!(!preview_within_limit(
            PreviewCategory::Text,
            PREVIEW_TEXT_MAX_BYTES + 1
        ));
        assert!(preview_within_limit(
            PreviewCategory::Delimited { tab: true },
            PREVIEW_CSV_MAX_BYTES
        ));
    }
}
