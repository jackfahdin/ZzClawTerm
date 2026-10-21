//! File recognition shared by the document editor and the file browser.

pub struct DocumentLanguage {
    pub id: &'static str,
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
}

// Only languages with bundled highlight queries belong here. The UI integration
// tests verify that every entry has an enabled parser and produces highlights.
pub const DOCUMENT_LANGUAGES: &[DocumentLanguage] = &[
    DocumentLanguage {
        id: "astro",
        extensions: &["astro"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "bash",
        extensions: &["sh", "bash"],
        filenames: &[
            ".bashrc",
            ".bash_profile",
            ".bash_login",
            ".bash_logout",
            ".bash_aliases",
            ".profile",
        ],
    },
    DocumentLanguage {
        id: "c",
        extensions: &["c", "h"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "cpp",
        extensions: &[
            "cpp", "hpp", "cc", "hh", "cxx", "hxx", "c++", "h++", "ipp", "tpp",
        ],
        filenames: &[],
    },
    DocumentLanguage {
        id: "css",
        extensions: &["css", "scss"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "diff",
        extensions: &["diff", "patch"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "ejs",
        extensions: &["ejs"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "elixir",
        extensions: &["ex", "exs"],
        filenames: &["mix.lock"],
    },
    DocumentLanguage {
        id: "erb",
        extensions: &["erb"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "go",
        extensions: &["go"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "html",
        extensions: &["html", "htm"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "java",
        extensions: &["java"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "javascript",
        extensions: &["js", "jsx", "mjs", "cjs"],
        filenames: &[],
    },
    // JSDoc is used by JavaScript/TypeScript injections, not a standalone file type.
    DocumentLanguage {
        id: "jsdoc",
        extensions: &[],
        filenames: &[],
    },
    DocumentLanguage {
        id: "json",
        extensions: &["json", "jsonc"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "kotlin",
        extensions: &["kt", "kts", "ktm"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "lua",
        extensions: &["lua"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "make",
        extensions: &["mk", "mak"],
        filenames: &["makefile", "gnumakefile"],
    },
    DocumentLanguage {
        id: "markdown",
        extensions: &["md", "markdown", "mdown", "mkd", "mdx"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "php",
        extensions: &["php", "php3", "php4", "php5", "phtml"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "python",
        extensions: &["py", "pyi", "pyw"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "ruby",
        extensions: &["rb", "rake", "gemspec"],
        filenames: &["gemfile", "rakefile", "guardfile", "brewfile", ".irbrc"],
    },
    DocumentLanguage {
        id: "rust",
        extensions: &["rs"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "scala",
        extensions: &["scala", "sc", "sbt"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "sql",
        extensions: &["sql"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "svelte",
        extensions: &["svelte"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "toml",
        extensions: &["toml"],
        filenames: &["cargo.lock", "uv.lock", "poetry.lock", "pdm.lock"],
    },
    DocumentLanguage {
        id: "tsx",
        extensions: &["tsx"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "typescript",
        extensions: &["ts", "mts", "cts"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "yaml",
        extensions: &["yaml", "yml"],
        filenames: &[],
    },
    DocumentLanguage {
        id: "zig",
        extensions: &["zig", "zon"],
        filenames: &[],
    },
];

pub fn document_language_for_path(path: &str) -> Option<&'static DocumentLanguage> {
    let filename = path.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    let extension = filename.rsplit_once('.').map(|(_, extension)| extension);
    DOCUMENT_LANGUAGES.iter().find(|language| {
        language.filenames.contains(&filename.as_str())
            || extension.is_some_and(|extension| language.extensions.contains(&extension))
    })
}

#[cfg(test)]
mod tests {
    use super::document_language_for_path;

    #[test]
    fn recognizes_source_variants_and_distinguishes_tsx() {
        for (path, expected) in [
            ("/srv/component.tsx", "tsx"),
            ("/srv/component.ts", "typescript"),
            ("/srv/module.mts", "typescript"),
            ("/srv/module.cjs", "javascript"),
            ("/srv/module.mjs", "javascript"),
            ("/srv/main.cc", "cpp"),
            ("/srv/header.hxx", "cpp"),
            ("/srv/types.pyi", "python"),
            ("/srv/README.markdown", "markdown"),
            ("/srv/settings.jsonc", "json"),
            ("/srv/main.RS", "rust"),
            (r"C:\src\QUERY.SQL", "sql"),
            ("view.html.erb", "erb"),
        ] {
            assert_eq!(
                document_language_for_path(path).map(|language| language.id),
                Some(expected),
                "{path}"
            );
        }
    }

    #[test]
    fn recognizes_extensionless_build_files_and_shell_profiles_by_basename() {
        for (path, expected) in [
            ("/home/user/.bashrc", "bash"),
            ("/home/user/.bash_profile", "bash"),
            ("/home/user/.profile", "bash"),
            ("/src/Makefile", "make"),
            (r"C:\src\GNUmakefile", "make"),
            ("/src/Gemfile", "ruby"),
            ("/src/Cargo.lock", "toml"),
            ("/src/mix.lock", "elixir"),
        ] {
            assert_eq!(
                document_language_for_path(path).map(|language| language.id),
                Some(expected),
                "{path}"
            );
        }
    }

    #[test]
    fn unsupported_files_do_not_borrow_an_unrelated_grammar() {
        for path in [
            "image.svg",
            "config.xml",
            "Dockerfile",
            "script.ps1",
            "settings.ini",
            "main.cs",
            "main.swift",
            "schema.proto",
            "CMakeLists.txt",
            "query.graphql",
            "settings.json5",
            "page.vue",
            ".zshrc",
            "unknown.bin",
            "",
            "/src.js/unknown",
            "rust",
            "json",
        ] {
            assert!(document_language_for_path(path).is_none(), "{path}");
        }
    }
}
